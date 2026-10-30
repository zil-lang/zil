#!/usr/bin/env python3
"""Read-only independent arithmetic/evidence audit; never imports ZIL runners.

The output is a reviewer receipt outside the extracted evidence directory.
Installed toolchain identities remain recorded CI evidence, not re-attested here.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import random
import re
import subprocess
import tarfile
import zipfile

BASE = "a715f828261ff4b6ef9c8125623064360745aae4"
BASE_TREE = "0221fa705e8dc926c54f6b85f2dc9ef3af98559a"
TREE = "b583bc62e4aa39b206afb27f76f2f8262120fe9d"
CASES = {"tree4x2": .05, "tree32x8": .15, "tree256x8": .30, "tree1024x8": .30, "retained": .20}
REQUESTS = {"cold_process": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}
SNAPSHOTS = {"cold_process": "base", "unchanged": "base", "private_edit": "private", "public_edit": "public"}
NUMBER_COMPARISONS = 0
MAX_ABSOLUTE_DIFFERENCE = 0.
WINDOWS = [
    ("o0-bridge", "build", "o0-bridge-source-object-no-query", "summary.json"),
    ("rust-o0-cold", "build", "rust-o0-cold-build", "build-summary.json"),
    ("rust-o0-cold", "runtime", "rust-o0-cold-runtime", "runtime-summary.json"),
    ("rust-o0-incremental", "build", "rust-o0-incremental-build", "build-summary.json"),
    ("rust-o0-incremental", "runtime", "rust-o0-incremental-runtime", "runtime-summary.json"),
    ("rust-o2-closed", "build", "rust-o2-closed-build", "build-summary.json"),
    ("rust-o2-closed", "runtime", "rust-o2-closed-runtime", "runtime-summary.json"),
    ("o2-direct-quality", "runtime", "baseline-closed-o2-1x1-vs-candidate-closed-o2-8x4", "summary.json"),
]


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def records(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def require(ok, message):
    if not ok:
        raise ValueError(message)


def median(values):
    ordered = sorted(values)
    n = len(ordered)
    return ordered[n // 2] if n % 2 else (ordered[n // 2 - 1] + ordered[n // 2]) / 2


def check_numbers(expected, observed, at=""):
    global NUMBER_COMPARISONS, MAX_ABSOLUTE_DIFFERENCE
    if isinstance(expected, dict):
        for key, value in expected.items():
            require(key in observed, "Missing reported statistic " + at + "/" + key)
            check_numbers(value, observed[key], at + "/" + key)
    elif isinstance(expected, list):
        require(len(expected) == len(observed), "Different statistic length " + at)
        for index, (a, b) in enumerate(zip(expected, observed)):
            check_numbers(a, b, at + "/" + str(index))
    else:
        require(type(observed) in (int, float) and math.isfinite(observed)
                and math.isclose(expected, observed, rel_tol=2e-12, abs_tol=1e-12),
                f"Statistic differs {at}: independent={expected!r}, reported={observed!r}")
        NUMBER_COMPARISONS += 1
        MAX_ABSOLUTE_DIFFERENCE = max(MAX_ABSOLUTE_DIFFERENCE, abs(expected - observed))


def archive_binding(root, archive, metadata_path, candidate):
    metadata = read(metadata_path)
    zip_sha = digest(archive)
    require(metadata["id"] == 12993022904 and metadata["workflow_run"]["id"] == 41086451087
            and metadata["workflow_run"]["head_sha"] == candidate
            and metadata["digest"] == "sha256:" + zip_sha
            and zip_sha == "18f2acc10acc9c9e508a92fb06beb63f5f9d331fe34a945342860c56ab894fcc",
            "Downloaded artifact differs from the exact CI artifact metadata")
    found, regular, byte_count = False, 0, 0
    with zipfile.ZipFile(archive) as z:
        claimed_tar = z.read("compile4-evidence.sha256").decode().split()[0]
        with z.open("compile4-evidence.tar.gz") as stream:
            tar_sha = hashlib.file_digest(stream, "sha256").hexdigest()
        require(tar_sha == claimed_tar, "Inner tar differs from its CI checksum receipt")
        with z.open("compile4-evidence.tar.gz") as stream, tarfile.open(fileobj=stream, mode="r|gz") as tar:
            names = set()
            for item in tar:
                require(item.name not in names and not Path(item.name).is_absolute() and ".." not in Path(item.name).parts,
                        "Duplicate or unsafe archive member")
                names.add(item.name)
                require(item.isdir() or item.isfile(), "Unexpected nonregular archive member")
                if item.isfile():
                    regular += 1
                    byte_count += item.size
                if item.name == "compile4-evidence/files.json":
                    with tar.extractfile(item) as file:
                        manifest_sha = hashlib.file_digest(file, "sha256").hexdigest()
                    require(manifest_sha == digest(root / "files.json"), "Extracted file manifest differs from authenticated archive")
                    found = True
    require(found, "Authenticated archive lacks the final evidence manifest")
    return {"artifact_id": metadata["id"], "workflow_run_id": metadata["workflow_run"]["id"],
            "head_sha": candidate, "zip_sha256": zip_sha, "tar_sha256": tar_sha,
            "metadata_sha256": digest(metadata_path), "archived_file_manifest_sha256": manifest_sha,
            "regular_files": regular, "total_regular_bytes": byte_count,
            "status": "DownloadedZIP_Metadata_InnerTar_ExtractedFileManifestMatch"}


def statistics_from_rows(rows, request_weights, runtime, resamples, seed):
    """Explicitly form two-observation block means, then median block cells."""
    blocks = (0, 1, 2)
    samples = defaultdict(list)
    for row in rows:
        request = "runtime" if runtime else row["request"]
        samples[(row["block"], row["case"], request, row["arm"])].append(
            (row["wall_seconds"], row["cpu_user"] + row["cpu_system"]))
    require(all(len(values) == 2 for values in samples.values()), "Every block/case/request/arm needs exactly two samples")
    means = {key: tuple(math.fsum(pair[k] for pair in values) / 2 for k in (0, 1))
             for key, values in samples.items()}
    request_weights = {"runtime": 1.} if runtime else request_weights

    def calculate(selected, metric):
        case_totals = {case: {arm: 0. for arm in "AB"} for case in CASES}
        request_totals = {req: {arm: 0. for arm in "AB"} for req in request_weights}
        costs = {}
        for case, case_weight in CASES.items():
            for request, request_weight in request_weights.items():
                values = {}
                for arm in "AB":
                    value = median([means[(block, case, request, arm)][metric] for block in selected])
                    values[arm] = value
                    case_totals[case][arm] += request_weight * value
                    request_totals[request][arm] += case_weight * value
                values["B_over_A"] = values["B"] / values["A"]
                costs[case + "/" + request] = values
        totals = {arm: math.fsum(CASES[case] * values[arm] for case, values in case_totals.items()) for arm in "AB"}
        totals["B_over_A"] = totals["B"] / totals["A"]
        for values in [*case_totals.values(), *request_totals.values()]:
            values["B_over_A"] = values["B"] / values["A"]
        result = {**totals, "by_case": case_totals}
        if not runtime:
            result["by_request"] = request_totals
        return result, costs

    wall, costs = calculate(blocks, 0)
    cpu, _ = calculate(blocks, 1)
    rng = random.Random(seed)
    bootstrap = sorted(calculate(rng.choices(blocks, k=3), 0)[0]["B_over_A"] for _ in range(resamples))
    interval = [bootstrap[int(resamples * .025) - 1], bootstrap[int(resamples * .975) - 1]]
    return {"wall": wall, "cpu": cpu, "exploratory_paired_block_bootstrap95": interval,
            "by_case_request": costs,
            "block_ratios": [calculate([block], 0)[0]["B_over_A"] for block in blocks]}


class Audit:
    def __init__(self, root, candidate, repo):
        self.root, self.candidate = root.resolve(), candidate
        self.repo = repo.resolve()
        self.result = read(self.root / "result.json")
        self.original = Path(self.result["original_evidence_root"])
        self.checked_logs = self.checked_native = 0
        self.verified_files = 0
        self.hosts = []
        self.plans = {}
        self.artifacts = {}
        self.test_counts = []
        self.git_cache = {}

    def git_digest(self, name):
        if name not in self.git_cache:
            data = subprocess.check_output(["git", "-C", str(self.repo), "show", self.candidate + ":" + name])
            self.git_cache[name] = hashlib.sha256(data).hexdigest()
        return self.git_cache[name]

    def local(self, value):
        path = Path(value)
        if path.is_absolute():
            path = path.relative_to(self.original)
        require(".." not in path.parts, "Path escapes evidence: " + str(value))
        return self.root / path

    def header(self):
        require(self.result["status"] == "Complete_Exploratory_NotAProductGate", "CI is incomplete; no scores")
        require(self.result["candidate_revision"] == self.candidate, "Unexpected candidate commit")
        require(self.result["base_revision"] == BASE and self.result["base_crates_tree"] == BASE_TREE
                and self.result["candidate_crates_tree"] == TREE, "Unexpected source identity")
        manifest = read(self.root / "manifest.json")
        require(manifest["run_id"] == "compile5-round1" and manifest["base_revision"] == BASE
                and manifest["candidate_crates_tree"] == TREE and manifest["blocks"] == 3
                and manifest["seed"] == 20261030 and manifest["llvm_version"] == "23.1.4"
                and manifest["rust_version"] == "1.99.1", "Changed frozen experiment")
        require(self.git_digest("tools/research/compile-performance/compile4-run.json") == self.result["manifest_sha256"]
                and self.git_digest("tools/research/compile-performance/compile4_ci.py") == self.result["runner_sha256"],
                "CI manifest/runner differs from the measured source commit")
        index = read(self.root / "files.json")
        actual = {str(p.relative_to(self.root)) for p in self.root.rglob("*") if p.is_file() and p.name != "files.json"}
        require(actual == set(index), "Evidence file set differs from the final manifest")
        for name, spec in index.items():
            path = self.local(name)
            require(path.stat().st_size == spec["bytes"] and digest(path) == spec["sha256"], "Evidence checksum mismatch: " + name)
        self.verified_files = len(index)
        expected_windows = [row[2] for row in WINDOWS]
        require([row["name"] for row in self.result["windows"]] == expected_windows, "Wrong window coverage/order")
        for window in self.result["windows"]:
            require(digest(self.local(window["summary"])) == window["sha256"], "Unbound summary")
        builds = self.builds = read(self.root / "compiler-builds.json")
        for label, commit in (("baseline", BASE), ("candidate", self.candidate)):
            row = builds[label]
            path = self.local(row["path"])
            require(row["source_commit"] == commit and digest(path) == row["sha256"]
                    and path.stat().st_size == row["bytes"], "Compiler build binding mismatch")
        for receipt_path in sorted((self.root / "logs").glob("*.json")):
            row = read(receipt_path)
            require(row["status"] == "Pass" and row["exit"] == 0 and row["timeout"] is False,
                    "CI command failed: " + row["label"])
            for stream in ("stdout", "stderr"):
                require(digest(self.local(row[stream])) == row[stream + "_sha256"], "Changed CI command log")
            if row["label"].startswith("candidate-tests-"):
                group_index = int(row["label"].removeprefix("candidate-tests-"))
                group = manifest["tests"][group_index]
                counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", self.local(row["stdout"]).read_text())
                require(len(counts) == len(group["targets"]) + int(group["lib"])
                        and all(int(passed) > 0 and int(failed) == 0 for passed, failed in counts),
                        "A selected Rust test target did not report nonempty success")
                require([row["argv"][i + 1] for i, arg in enumerate(row["argv"][:-1]) if arg == "--test"] == group["targets"]
                        and ("--lib" in row["argv"]) is group["lib"], "Different test target invocation")
                labels = re.findall(r"(?m)^\s+Running (unittests|tests/[a-zA-Z0-9_]+\.rs)\s", self.local(row["stderr"]).read_text())
                labels = ["lib" if label == "unittests" else Path(label).stem for label in labels]
                expected_labels = set(group["targets"]) | ({"lib"} if group["lib"] else set())
                require(len(labels) == len(counts) and set(labels) == expected_labels, "Cannot bind Rust pass counts to actual target execution")
                self.test_counts.append({"package": group["package"], "targets": group["targets"], "lib": group["lib"],
                                         "executions": [{"target": label, "passed": int(passed), "failed": int(failed)}
                                                        for label, (passed, failed) in zip(labels, counts)]})
        require(len(self.test_counts) == len(manifest["tests"]), "Missing candidate test groups")

    def plan(self, folder):
        if folder in self.plans:
            return self.plans[folder]
        path = self.root / "windows" / folder / "plan/plan.json"
        plan = read(path)
        require({case["id"]: case["weight"] for case in plan["cases"]} == CASES
                and len(plan["cases"]) == 5 and plan["blocks"] == 3 and plan["seed"] == 20261030,
                "Case/weight/block/seed drift: " + folder)
        require(plan["runner_source_commit"] == self.candidate, "Runner/candidate identity drift")
        runner = "bench.py" if folder == "o0-bridge" else "runtime_bench.py" if folder == "o2-direct-quality" else "rust_bench.py"
        require(plan["runner_sha256"] == self.git_digest("tools/research/compile-performance/" + runner), "Changed frozen benchmark runner")
        for name, expected in plan.get("helper_sha256", plan.get("dependencies_sha256", {})).items():
            require(self.git_digest(name) == expected, "Changed frozen helper/oracle: " + name)
        for arm, identity in plan["source_identity"].items():
            require(identity["commit"] == (BASE if arm == "A" else self.candidate)
                    and identity["crates_tree"] == (BASE_TREE if arm == "A" else TREE), "Plan source binding drift")
            compiler = self.builds["baseline" if arm == "A" else "candidate"]
            require(plan["binaries"][arm]["sha256"] == compiler["sha256"]
                    and plan["binaries"][arm]["path"] == compiler["path"], "Plan compiler differs from built binary")
        cases = list(CASES)
        rng, order = random.Random(20261030), []
        for block in range(3):
            names = cases.copy()
            rng.shuffle(names)
            for name in names:
                for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"):
                    row = {"block": block, "case": name, "position": position, "arm": arm}
                    if folder == "o0-bridge":
                        row["phase"] = "build"
                    order.append(row)
        require(plan["order"] == order, "Plan sampling schedule differs: " + folder)
        if folder == "o0-bridge":
            require(plan["phases"] == ["build"] and plan["profile"] == "O0"
                    and plan["object_cache"] is True and plan["units"] == 8 and plan["jobs"] == 4
                    and plan["request_weights"] == REQUESTS, "Bridge contract drift")
        elif folder.startswith("rust-"):
            incremental = folder == "rust-o0-incremental"
            closed = folder == "rust-o2-closed"
            require(plan["profile"] == ("O2" if closed else "O0")
                    and plan["cache_mode"] == ("incremental" if incremental else "cold")
                    and plan["query_cache"] is incremental and plan["closed_executable"] is closed
                    and plan["rust_profile"] == plan["rust_lto"] == "native"
                    and plan["units"] == 8 and plan["jobs"] == 4
                    and plan["request_weights"] == (REQUESTS if incremental else {"cold_process": 1.}),
                    "Rust comparison contract drift")
        else:
            require(plan["profile"] == "O2" and plan["object_cache"] is False and plan["timings"] is False
                    and plan["build_config"] == {
                        "A": {"profile": "O2", "units": 1, "jobs": 1, "closed_executable": True},
                        "B": {"profile": "O2", "units": 8, "jobs": 4, "closed_executable": True}}, "Runtime-quality contract drift")
        fixture_root = path.parent / "corpus" if folder == "o0-bridge" else path.parent
        for name, expected in plan["fixtures_sha256"].items():
            require(digest(fixture_root / name) == expected, "Frozen fixture mismatch: " + name)
        self.hosts.append({key: plan["host"][key] for key in ("cpu_model", "cpu_max", "memory_max")})
        self.plans[folder] = (path, plan)
        return path, plan

    def command(self, folder, plan, row):
        work = Path(row["work"])
        binary = plan["binaries"][row["arm"]]["path"]
        if folder == "o0-bridge":
            return [binary, "build", str(work / "src/main.zil"), "--json", "--cache-dir", str(work / "source-candidates"),
                    "--clang", plan["binaries"]["clang"]["path"], "--opt", "O0", "--codegen-units", "8", "--codegen-jobs", "4",
                    "-o", str(work / "program"), "--object-cache", str(work / "objects")]
        if row["arm"] == "A":
            argv = [binary, str(work / "src/main.rs"), "--edition=2024", "--crate-name", "zil_cp08_ref", "-Awarnings",
                    "--target", "x86_64-unknown-linux-gnu", "-C", "target-cpu=x86-64", "-C", "opt-level=" + plan["profile"][1:],
                    "-C", "debuginfo=0", "-C", "overflow-checks=on", "-C", "panic=abort", "-C", "embed-bitcode=no", "-o", str(work / "program")]
            if plan["cache_mode"] == "incremental":
                argv += ["-C", "incremental=" + str(work / "rust-incremental")]
            return argv
        argv = [binary, "build", str(work / "src/main.zil"), "-o", str(work / "program"), "--json", "--backend", "llvm-api",
                "--clang", plan["binaries"]["clang"]["path"], "--opt", plan["profile"], "--codegen-units", "8", "--codegen-jobs", "4"]
        if plan["cache_mode"] == "incremental":
            argv += ["--cache-dir", str(work / "source-candidates"), "--object-cache", str(work / "objects"), "--query-cache", str(work / "queries")]
        if plan["closed_executable"]:
            argv += ["--closed-executable"]
        return argv

    def logs(self, row, logroot, native=False):
        for stream in ("stdout", "stderr"):
            path = logroot / row[stream]
            require(digest(path) == row[stream + "_sha256"], "Raw log mismatch: " + str(path))
            if native and stream == "stderr":
                require(path.stat().st_size == 0, "Native stderr is nonempty")
        self.checked_logs += 1

    def native(self, row, logroot, planroot, input_path, expected_path):
        require(row.get("pass", row.get("matches")) is True and row["exit"] == 0 and row["timeout"] is False,
                "Failed native oracle/run")
        require(row["input"] == input_path and row["expected"] == expected_path, "Wrong native snapshot")
        require(row["input_sha256"] == row["input_after_sha256"] == digest(planroot / input_path)
                and row["stdout_sha256"] == row["expected_stdout_sha256"] == digest(planroot / expected_path),
                "Native oracle/input digest mismatch")
        require((logroot / row["stdout"]).read_bytes() == (planroot / expected_path).read_bytes(), "Native output bytes differ")
        if "executable" in row:
            require(row["executable"] == row["executable_after"], "Executable changed during execution")
        self.logs(row, logroot, native=True)
        self.checked_native += 1

    def sample_rows(self, folder, kind, planpath, plan):
        directory = planpath.parent.parent / kind
        path = directory / "records.jsonl"
        rows = records(path)
        preflight_rows = kind == "preflight"
        summary_receipt = directory / ("result.json" if preflight_rows else "complete.json")
        receipt = read(summary_receipt)
        preflight = planpath.parent.parent / ("build/result.json" if folder == "o2-direct-quality" else "preflight/result.json")
        pre = read(preflight)
        require(pre["pass"] is True and not pre["failures"] and pre["plan_sha256"] == digest(planpath), "Preflight does not pass")
        require(receipt["records_sha256"] == digest(path) and receipt["plan_sha256"] == digest(planpath)
                and receipt["rows"] == len(rows) and not receipt["failures"] and receipt.get("pass", True) is True,
                "Completion receipt mismatch")
        prekey = "build_receipt_sha256" if folder == "o2-direct-quality" else "preflight_sha256"
        if not preflight_rows:
            require(receipt[prekey] == digest(preflight), "Completion/preflight mismatch")
        order = [{"case": case["id"], "arm": arm, "block": 0, "position": position,
                  **({"phase": "build"} if folder == "o0-bridge" else {})}
                 for case in plan["cases"] for position, arm in enumerate("AB")] if preflight_rows else plan["order"]
        expected = [{**seq, "request": req} for seq in order for req in plan["request_weights"]] if kind != "runtime" else order
        require(len(rows) == len(expected), "Missing or extra sample rows")
        cases = {case["id"]: case for case in plan["cases"]}
        for index, (row, scheduled) in enumerate(zip(rows, expected)):
            require(all(row[key] == value for key, value in scheduled.items()) and row["index"] == index,
                    "Sample order/index mismatch")
            require(row["plan_sha256"] == digest(planpath) and row["pass"] is True
                    and row["exit"] == 0 and row["timeout"] is False, "Invalid sample status or plan")
            require(all(type(row[key]) in (int, float) and math.isfinite(row[key]) and row[key] >= 0
                        for key in ("wall_seconds", "cpu_user", "cpu_system")) and row["wall_seconds"] > 0,
                    "Invalid timing sample")
            case = cases[row["case"]]
            if kind == "runtime":
                self.native(row, directory / "logs", planpath.parent, case["input"], case["expected"])
                require(row["executable"] == self.artifacts[folder][(row["case"], row["arm"])], "Runtime sample is not its preflight executable")
            else:
                self.logs(row, directory / "logs")
                require(row["argv"] == self.command(folder, plan, row), "Compiler command differs from frozen comparison")
                snapshot = SNAPSHOTS[row["request"]]
                require(row["snapshot"] == snapshot, "Wrong source snapshot")
                lang = "rust" if folder.startswith("rust-") and row["arm"] == "A" else "zil"
                source = planpath.parent / case["root"] / snapshot / lang
                source_hashes = {str(file.relative_to(source)): digest(file) for file in source.rglob("*") if file.is_file()}
                require(source_hashes and source_hashes == row["source_before"] == row["source_after"], "Source snapshot differs")
                require(len(row["checks"]) == len(case["inputs"]), "Missing native oracle rows")
                for check, sample in zip(row["checks"], case["inputs"]):
                    self.native(check, directory / "logs", planpath.parent, sample["path"], sample["expected"][snapshot])
                    if "executable" in check:
                        require(check["executable"] == row["executable"], "Oracle did not run the built executable")
                if lang == "zil":
                    report = read(directory / "logs" / row["stdout"])
                    require(report["status"] == "Success" and report["verification_runs"] == 1
                            and report.get("timings") is None and report["native_codegen"]["backend"] == "llvm-api",
                            "ZIL current Verify/backend/timings contract differs")
                    native = report["native_codegen"]
                    cache = native["object_cache"]
                    incremental = folder in ("o0-bridge", "rust-o0-incremental")
                    require(bool(cache.get("enabled")) is incremental, "Unexpected object-cache mode")
                    if incremental and row["request"] in ("cold_process", "unchanged"):
                        require(cache["hits"] == (0 if row["request"] == "cold_process" else native["actual_units"]), "Object-cache request contract differs")
                    query = report.get("query_cache") or {}
                    if folder == "rust-o0-incremental":
                        hit = row["request"] == "unchanged"
                        require(query.get("requested") is True and query.get("state") == ("hit" if hit else "miss")
                                and query.get("stored") is (not hit), "Query cache request contract differs")
                    else:
                        require(not query.get("requested"), "Unexpected query cache")
                    require(bool(native.get("closed_executable", False)) is (folder == "rust-o2-closed"), "Wrong linkage mode")
        return rows

    def preflight(self, folder, planpath, plan):
        if folder in self.artifacts:
            return
        cases = {case["id"]: case for case in plan["cases"]}
        artifacts = {}
        if folder != "o2-direct-quality":
            self.sample_rows(folder, "preflight", planpath, plan)
            directory = planpath.parent.parent / "preflight"
            if folder == "o0-bridge":
                self.artifacts[folder] = artifacts
                return
            path = directory / "artifacts.json"
            rows = read(path)
            require(read(directory / "result.json")["artifacts_sha256"] == digest(path), "Preflight executable list digest differs")
        else:
            directory = planpath.parent.parent / "build"
            path = directory / "builds.jsonl"
            rows = records(path)
            receipt = read(directory / "result.json")
            require(receipt["pass"] is True and not receipt["failures"] and receipt["rows"] == 10
                    and receipt["builds_sha256"] == digest(path) and receipt["plan_sha256"] == digest(planpath),
                    "Quality build receipt mismatch")
            for row in rows:
                case = cases[row["case"]]
                require(row["pass"] is True and row["build_process"]["exit"] == 0 and row["build_process"]["timeout"] is False
                        and row["source_before"] == row["source_after"] == case["source_sha256"], "Quality preflight build failed/source drift")
                self.logs(row, directory / "logs")
                report = read(directory / "logs" / row["stdout"])
                require(report["status"] == "Success" and report["verification_runs"] == 1 and report.get("timings") is None
                        and report["native_codegen"]["backend"] == "llvm-api" and report["native_codegen"]["closed_executable"] is True,
                        "Quality current Verify/backend/linkage contract differs")
        expected = [(case["id"], arm) for case in plan["cases"] for arm in "AB"]
        require([(row["case"], row["arm"]) for row in rows] == expected, "Preflight executable coverage/order differs")
        for row in rows:
            case = cases[row["case"]]
            executable = row["executable"]
            binary = self.local(executable["path"])
            require(digest(binary) == executable["sha256"] and binary.stat().st_size == executable["size"], "Retained executable differs from preflight")
            for label, input_name, expected_name in (("startup", "inputs/empty.bin", "expected/empty.bin"),
                                                     ("warmup", case["input"], case["expected"])):
                self.native(row[label], directory / "logs", planpath.parent, input_name, expected_name)
                require(row[label]["executable"] == executable, "Startup/warmup executable differs")
            artifacts[(row["case"], row["arm"])] = executable
        self.artifacts[folder] = artifacts

    def run(self):
        self.header()
        output = {"candidate_revision": self.candidate, "base_revision": BASE,
                  "status": "IndependentArithmeticAndArchivedEvidencePass", "windows": []}
        for folder, kind, name, summary_name in WINDOWS:
            planpath, plan = self.plan(folder)
            self.preflight(folder, planpath, plan)
            rows = self.sample_rows(folder, kind, planpath, plan)
            summary = read(planpath.parent.parent / summary_name)
            require(summary["coverage_complete"] is True and not summary["failures"] and summary["product_gates"] == "NotRun", "Summary incorrectly claims completion")
            stats = statistics_from_rows(rows, plan.get("request_weights", {}), kind == "runtime",
                                         5000 if folder == "o2-direct-quality" else 2000, plan["seed"])
            reported = summary["by_phase"]["build"] if folder == "o0-bridge" else summary
            check_numbers({key: stats[key] for key in ("wall", "cpu", "exploratory_paired_block_bootstrap95")}, reported, name)
            if folder == "o0-bridge":
                check_numbers(stats["by_case_request"], reported["by_case_request"], name + "/cells")
                digests = defaultdict(set)
                for row in rows:
                    require(row["executable_sha256"] == row["executable_after_sha256"], "Bridge executable mutated")
                    require(type(row["executable_bytes"]) is int and row["executable_bytes"] > 0, "Invalid recorded bridge executable size")
                    digests[(row["case"], row["snapshot"])].add((row["executable_sha256"], row["executable_bytes"]))
                same = all(len(values) == 1 for values in digests.values())
                require(same == reported["same_binary_for_each_case_snapshot"], "Bridge binary identity claim differs")
                stats["same_binary_for_each_case_snapshot"] = same
                stats["binary_comparison_cells"] = len(digests)
                stats["bridge_binary_identity_scope"] = "Recorded ELF SHA256/size consistency only: bridge executables were deleted by the frozen runner; no post-run ELF bytes rehash is claimed"
            output["windows"].append({"name": name, "rows": len(rows), **stats})
        require(all(host == self.hosts[0] for host in self.hosts), "Recorded host/quota changes between windows")
        output.update(files_verified=self.verified_files, raw_log_pairs_verified=self.checked_logs,
                      native_oracle_or_runtime_rows_verified=self.checked_native, recorded_host=self.hosts[0],
                      candidate_tests=self.test_counts,
                      numeric_comparisons=NUMBER_COMPARISONS, maximum_absolute_statistic_difference=MAX_ABSOLUTE_DIFFERENCE,
                      scope="Independent arithmetic and archived file/row/log/oracle checks only; no compiler or benchmark execution, no product gate, no re-attestation of unarchived host tools")
        return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--repo", type=Path, default=Path("/workspace/scratch/fec853c597a1/zil"))
    parser.add_argument("--artifact-zip", type=Path)
    parser.add_argument("--artifact-metadata", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    require(args.root.resolve() not in args.out.resolve().parents, "Write reviewer output outside immutable evidence")
    try:
        value = Audit(args.root, args.candidate, args.repo).run()
        if args.artifact_zip or args.artifact_metadata:
            require(args.artifact_zip is not None and args.artifact_metadata is not None, "Both archive and artifact metadata are required")
            value["archive_binding"] = archive_binding(args.root, args.artifact_zip, args.artifact_metadata, args.candidate)
        value["reviewer_script_sha256"] = digest(Path(__file__))
    except (OSError, ValueError, KeyError, TypeError, ZeroDivisionError, subprocess.SubprocessError) as error:
        value = {"status": "IndependentAuditFailed_NoScore", "error": repr(error)}
    args.out.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: value[key] for key in value if key != "windows"}, ensure_ascii=False, indent=2))
    return 0 if value["status"] == "IndependentArithmeticAndArchivedEvidencePass" else 1


if __name__ == "__main__":
    raise SystemExit(main())

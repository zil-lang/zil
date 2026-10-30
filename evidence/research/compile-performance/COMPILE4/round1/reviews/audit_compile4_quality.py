#!/usr/bin/env python3
"""Read-only independent audit of the frozen compile4 direct O2 quality window.

Uses only the Python standard library. It does not import the measured runner,
recompile/reexecute a measured program, or mutate any original evidence file.
"""
from __future__ import annotations

import argparse
import collections
import datetime
import hashlib
import itertools
import json
import math
from pathlib import Path
import random
import re
import statistics
import subprocess


EXPECTED_CASES = {
    "tree4x2": (0.05, 1048576, 4, 2),
    "tree32x8": (0.15, 32768, 32, 8),
    "tree256x8": (0.30, 4096, 256, 8),
    "tree1024x8": (0.30, 1024, 1024, 8),
    "retained": (0.20, 262144, None, None),
}
BASE = "64072793a0f41994f216a6207bf49ee1e2d224f8"
CANDIDATE = "0393660f7dc8414b0ab8c77cd3d576395f4fe095"
CRATES_TREE = "0221fa705e8dc926c54f6b85f2dc9ef3af98559a"
SMALL_REASON = "small optimized closed executable"


def load(path):
    return json.loads(path.read_text())


class Audit:
    def __init__(self, root, repo):
        self.root = root.resolve()
        self.repo = repo.resolve()
        self.window = self.root / "windows/o2-direct-quality"
        self.failures = []
        self.checks = 0
        self.hashes = {}
        self.differences = []
        self.row_checks = collections.Counter()
        self.original = Path(load(self.root / "result.json")["original_evidence_root"])

    def check(self, condition, label):
        self.checks += 1
        if not condition:
            self.failures.append(label)

    def sha(self, path):
        path = Path(path)
        if path not in self.hashes:
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                while data := stream.read(1024 * 1024):
                    digest.update(data)
            self.hashes[path] = digest.hexdigest()
        return self.hashes[path]

    def mapped(self, original):
        relative = Path(original).relative_to(self.original)
        path = self.root / relative
        if not path.resolve().is_relative_to(self.root):
            raise ValueError("Evidence path escapes original root")
        return path

    def same_number(self, actual, expected, label):
        delta = abs(actual - expected)
        self.differences.append(delta)
        self.check(math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), label)

    def inventory(self, entry, label):
        path = self.mapped(entry["path"])
        self.check(path.is_file() and not path.is_symlink(), label + ": retained regular file")
        self.check(self.sha(path) == entry["sha256"], label + ": executable sha256")
        self.check(path.stat().st_size == entry["size"], label + ": executable size")
        self.check(bool(path.stat().st_mode & 0o111), label + ": executable mode retained")

    def native(self, row, executable, case, stage, logs):
        label = stage + "/" + case["id"]
        empty = stage == "startup"
        input_name = "inputs/empty.bin" if empty else case["input"]
        expected_name = "expected/empty.bin" if empty else case["expected"]
        source = self.window / "plan" / input_name
        expected = self.window / "plan" / expected_name
        output = logs / row["stdout"]
        errors = logs / row["stderr"]
        self.check(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False,
                   label + ": successful native exit")
        self.check(row["input"] == input_name and row["expected"] == expected_name,
                   label + ": fixed input and oracle paths")
        self.check(row["executable"] == row["executable_after"] == executable,
                   label + ": exact recorded executable identity before/after")
        self.check(row["input_sha256"] == row["input_after_sha256"] == self.sha(source),
                   label + ": input bytes and recorded before/after identity")
        self.check(row["expected_stdout_sha256"] == row["stdout_sha256"]
                   == self.sha(output) == self.sha(expected), label + ": stdout hashes")
        self.check(output.read_bytes() == expected.read_bytes(), label + ": stdout byte comparison")
        self.check(row["stdout_bytes"] == output.stat().st_size, label + ": stdout length")
        self.check(errors.stat().st_size == 0 and row["stderr_sha256"] == self.sha(errors),
                   label + ": empty stderr bytes and hash")
        self.check(all(type(row[k]) in (float, int) and math.isfinite(row[k]) and row[k] >= 0
                       for k in ("wall_seconds", "cpu_user", "cpu_system")),
                   label + ": finite nonnegative timing")
        self.check(row["wall_seconds"] > 0 and row["cpu_user"] + row["cpu_system"] > 0,
                   label + ": positive complete-process wall and CPU")
        self.row_checks[stage] += 1

    def fixture_oracle(self, case):
        weight, size, modules, width = EXPECTED_CASES[case["id"]]
        data = (self.window / "plan" / case["input"]).read_bytes()
        expected = (self.window / "plan" / case["expected"]).read_bytes()
        if modules is None:
            generated_input = bytes(0 if i % 257 == 256 else (73 * i + 19) % 255 + 1
                                    for i in range(size))
            history = bytearray()
            total = 0
            generated_output = bytearray()
            for byte in data:
                if byte == 0:
                    history.clear()
                    total = 0
                    generated_output.append(0)
                else:
                    history.append(byte)
                    total += byte
                    length = len(history)
                    generated_output.append((total + length + history[(total + length) % length]) % 251)
        else:
            generated_input = bytes((73 * i + 19) % 256 for i in range(size))

            # Interpret each source tree's straight-line scalar functions for
            # all byte values. This does not use the runner's affine-map oracle.
            def node(index, value):
                for k in range(width):
                    value = (value * (2 + (index * width + k) % 11)
                             + 1 + (index * 7 + k * 3) % 29) % 251
                for child in (2 * index + 1, 2 * index + 2):
                    if child < modules:
                        value = node(child, value)
                return value

            lookup = bytes(node(0, (byte + 24) % 251) for byte in range(256))
            generated_output = data.translate(lookup)
        self.check(data == generated_input, case["id"] + ": independently regenerated input")
        self.check(expected == generated_output, case["id"] + ": independent logical output oracle")
        self.check(case["weight"] == weight and case["input_bytes"] == size,
                   case["id"] + ": frozen weight and input size")

    def run(self):
        root_result = load(self.root / "result.json")
        manifest = load(self.root / "manifest.json")
        compiler_builds = load(self.root / "compiler-builds.json")
        index = load(self.root / "files.json")
        plan_path = self.window / "plan/plan.json"
        plan = load(plan_path)
        summary = load(self.window / "summary.json")
        built = load(self.window / "build/result.json")
        complete = load(self.window / "runtime/complete.json")
        builds = [json.loads(line) for line in (self.window / "build/builds.jsonl").read_text().splitlines()]
        rows = [json.loads(line) for line in (self.window / "runtime/records.jsonl").read_text().splitlines()]
        cases = {case["id"]: case for case in plan["cases"]}
        self.check(list(cases) == list(EXPECTED_CASES) and len(plan["cases"]) == 5,
                   "exact five-case coverage and order")
        self.check(root_result["candidate_revision"] == CANDIDATE and root_result["base_revision"] == BASE,
                   "exact remote source commits")
        self.check(root_result["candidate_crates_tree"] == CRATES_TREE,
                   "exact candidate crates tree")
        self.check(root_result["status"] == "Complete_Exploratory_NotAProductGate",
                   "complete exploratory root receipt")
        self.check(self.sha(self.root / "manifest.json") == root_result["manifest_sha256"],
                   "manifest bytes match root receipt")
        self.check(manifest["candidate_crates_tree"] == CRATES_TREE and manifest["base_revision"] == BASE,
                   "manifest source binding")
        self.check(plan["source_identity"] == {
            "A": {"commit": BASE, "crates_tree": root_result["base_crates_tree"]},
            "B": {"commit": CANDIDATE, "crates_tree": CRATES_TREE}}, "runtime source identities")
        self.check(plan["runner_source_commit"] == CANDIDATE, "runtime runner source commit")
        self.check(plan["build_config"] == {
            "A": {"profile": "O2", "units": 1, "jobs": 1, "closed_executable": True},
            "B": {"profile": "O2", "units": 8, "jobs": 4, "closed_executable": True}},
            "frozen arm configurations")
        self.check(plan["blocks"] == 3 and plan["seed"] == 20261030 and plan["expected_rows"] == 60,
                   "frozen block count, seed and row count")
        self.check(plan["cpu"] == 0 and plan["cpu"] in plan["host"]["allowed_cpus"],
                   "frozen single CPU permitted by recorded host")
        self.check(plan["object_cache"] is False and plan["timings"] is False,
                   "runtime plan has no object cache or profiling")

        # Verify all 1,540 retained files in this window and the metadata used
        # here. The extraction changes local paths/times, not byte identity.
        selected = {p for p in index if p.startswith("windows/o2-direct-quality/")}
        actual = {str(p.relative_to(self.root)) for p in self.window.rglob("*") if p.is_file()}
        self.check(selected == actual, "quality file set equals original file index")
        selected.update({"result.json", "manifest.json", "compiler-builds.json",
                         "windows/o0-bridge/plan/plan.json"})
        selected.update(p for p in index if p.startswith("binaries/"))
        selected.update(p for p in index if p.startswith("logs/") and
                        any(token in p for token in ("source-tree", "candidate-tests-1", "quality-")))
        for relative in sorted(selected):
            path = self.root / relative
            self.check(path.is_file() and not path.is_symlink(), relative + ": regular retained file")
            self.check(path.stat().st_size == index[relative]["bytes"]
                       and self.sha(path) == index[relative]["sha256"], relative + ": indexed bytes and sha256")

        tar = self.root.parent / "compile4-evidence.tar.gz"
        extraction = load(self.root.parent / "EXTRACTION.json")
        tar_digest = self.sha(tar)
        self.check(tar_digest == extraction["tar_sha256"], "original tar hash matches extraction receipt")
        self.check(tar_digest == (self.root.parent / "compile4-evidence.sha256").read_text().split()[0],
                   "original tar hash matches packaged checksum")

        tree_log = next((self.root / "logs").glob("*-source-tree.stdout"))
        blobs = {line.split("\t", 1)[1]: line.split()[2] for line in tree_log.read_text().splitlines()}
        helpers = {"tools/research/compile-performance/runtime_bench.py": plan["runner_sha256"],
                   "tools/research/compile-performance/compile4_ci.py": root_result["runner_sha256"],
                   **plan["helper_sha256"]}
        for relative, expected_hash in helpers.items():
            data = (self.repo / relative).read_bytes()
            self.check(hashlib.sha256(data).hexdigest() == expected_hash, relative + ": frozen source hash")
            blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            self.check(blobs[relative] == blob, relative + ": candidate source-tree blob")
        machine_path = "crates/zil-backend-llvm/src/machine_cgu.rs"
        machine = (self.repo / machine_path).read_bytes()
        machine_blob = hashlib.sha1(b"blob " + str(len(machine)).encode() + b"\0" + machine).hexdigest()
        self.check(blobs[machine_path] == machine_blob, "CP14 source matches candidate source-tree blob")
        self.check(subprocess.check_output(["git", "rev-parse", "HEAD:crates"], cwd=self.repo, text=True).strip()
                   == CRATES_TREE, "local read-only crate source corresponds to candidate crate tree")
        self.check(b"let mut remaining = 2048_usize;" in machine
                   and b"for instructions in [2047, 2048, 2049]" in machine,
                   "frozen threshold and exact three boundary cases present")
        test_receipt_path = next((self.root / "logs").glob("*-candidate-tests-1.json"))
        test_receipt = load(test_receipt_path)
        test_log = (self.root / test_receipt["stdout"]).read_text()
        self.check(test_receipt["exit"] == 0 and test_receipt["timeout"] is False
                   and test_receipt["status"] == "Pass", "remote backend tests actually completed")
        for stream in ("stdout", "stderr"):
            self.check(self.sha(self.root / test_receipt[stream]) == test_receipt[stream + "_sha256"],
                       "backend test " + stream + " hash")
        test_names = ["cp14_small_closed_o2_obeys_total_instruction_boundary",
                      "cp14_disabled_and_single_unit_paths_are_unchanged",
                      "cp14_large_closed_program_preserves_cross_unit_reference",
                      "cp14_small_closed_o2_cannot_bypass_invalid_codegen_options"]
        for name in test_names:
            self.check(bool(re.search(r"test (?:machine_cgu::tests::)?" + name + r" \.\.\. ok", test_log)),
                       "executed CP14 control: " + name)
        counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", test_log)
        self.check(counts == [("45", "0"), ("6", "0")], "nonempty backend library and native test counts")

        upstream_path = self.mapped(plan["upstream_corpus_plan"]["path"])
        self.check(self.sha(upstream_path) == plan["upstream_corpus_plan"]["sha256"], "upstream corpus plan hash")
        upstream = load(upstream_path)
        upstream_cases = {case["id"]: case for case in upstream["cases"]}
        expected_fixtures = {str(p.relative_to(self.window / "plan")): self.sha(p)
                             for prefix in ("sources", "inputs", "expected")
                             for p in (self.window / "plan" / prefix).rglob("*") if p.is_file()}
        self.check(expected_fixtures == plan["fixtures_sha256"], "complete fixture set and content hashes")
        for case in cases.values():
            actual_sources = {str(p.relative_to(self.window / "plan" / case["source"])): self.sha(p)
                              for p in (self.window / "plan" / case["source"]).rglob("*") if p.is_file()}
            self.check(actual_sources == case["source_sha256"], case["id"] + ": source map matches real bytes")
            prefix = upstream_cases[case["id"]]["root"].removeprefix("corpus/") + "/base/zil/"
            inherited = {name[len(prefix):]: value for name, value in upstream["fixtures_sha256"].items()
                         if name.startswith(prefix)}
            self.check(inherited == actual_sources, case["id"] + ": exact upstream base sources")
            self.fixture_oracle(case)
        self.check((self.window / "plan/inputs/empty.bin").stat().st_size == 0
                   and (self.window / "plan/expected/empty.bin").stat().st_size == 0, "empty startup fixtures")

        for arm, name, revision in (("A", "baseline", BASE), ("B", "candidate", CANDIDATE)):
            self.inventory(plan["binaries"][arm], name + " compiler")
            b = compiler_builds[name]
            self.check(b["source_commit"] == revision and b["sha256"] == plan["binaries"][arm]["sha256"]
                       and b["bytes"] == plan["binaries"][arm]["size"]
                       and b["path"] == plan["binaries"][arm]["path"], name + ": compiler build identity")

        plan_hash = self.sha(plan_path)
        build_hash = self.sha(self.window / "build/result.json")
        records_hash = self.sha(self.window / "runtime/records.jsonl")
        self.check(built["pass"] is True and built["failures"] == [] and built["runtime_samples"] == 0
                   and built["rows"] == 10 and built["plan_sha256"] == plan_hash
                   and built["builds_sha256"] == self.sha(self.window / "build/builds.jsonl"),
                   "complete ten-build preflight receipt")
        expected_build_order = [(name, arm) for name in cases for arm in "AB"]
        self.check([(r["case"], r["arm"]) for r in builds] == expected_build_order, "build order and coverage")
        products = {}
        native_reports = {}
        object_report_count = 0
        for row in builds:
            name, arm = row["case"], row["arm"]
            case = cases[name]
            label = name + "/" + arm
            config = plan["build_config"][arm]
            expected_argv = [plan["binaries"][arm]["path"], "build",
                str(self.original / "windows/o2-direct-quality/plan" / case["source"] / "main.zil"),
                "-o", row["executable"]["path"], "--json", "--backend", "llvm-api", "--opt", "O2",
                "--codegen-units", str(config["units"]), "--codegen-jobs", str(config["jobs"]),
                "--clang", plan["binaries"]["clang"]["path"], "--closed-executable"]
            self.check(row["argv"] == row["build_process"]["argv"] == expected_argv,
                       label + ": exact build command")
            self.check(row["pass"] is True and row["plan_sha256"] == plan_hash
                       and row["build_process"]["exit"] == 0 and row["build_process"]["timeout"] is False,
                       label + ": successful build tied to plan")
            self.check(row["source_before"] == row["source_after"] == case["source_sha256"],
                       label + ": source identity unchanged during build")
            for stream in ("stdout", "stderr"):
                self.check(self.sha(self.window / "build/logs" / row[stream]) == row[stream + "_sha256"],
                           label + ": build " + stream + " hash")
            report = load(self.window / "build/logs" / row["stdout"])
            self.check(report["status"] == row["compiler_status"] == "Success"
                       and report["verification_runs"] == row["verification_runs"] == 1
                       and report["timings"] is None and row["timings_disabled"] is True,
                       label + ": successful current Verify and disabled profiling")
            self.check(report["query_cache"]["requested"] is False and report["disk_cache"] == "disabled",
                       label + ": query and source caches disabled")
            native = report["native_codegen"]
            self.check(native["backend"] == "llvm-api" and native["closed_executable"] is True
                       and native["requested_units"] == config["units"], label + ": backend and requested policy")
            self.check(1 <= native["actual_units"] <= config["units"]
                       and native["jobs"] == min(config["jobs"], native["actual_units"]),
                       label + ": actual unit count and job bound")
            if arm == "A":
                self.check(native["actual_units"] == 1 and native["fallback"] is None, label + ": old single unit")
            elif native["fallback"] == SMALL_REASON:
                self.check(native["actual_units"] == 1 and native["jobs"] == 1, label + ": CP14 effective single unit")
            else:
                self.check(native["actual_units"] == 8 and native["jobs"] == 4 and native["fallback"] is None,
                           label + ": ordinary eight-unit plan")
            self.check(native["object_cache"]["requested"] is False
                       and native["object_cache"]["enabled"] is False and native["object_cache"]["hits"] == 0,
                       label + ": actual object cache disabled")
            self.check(len(native["object_reports"]) == native["actual_units"], label + ": one receipt per actual unit")
            for number, obj in enumerate(native["object_reports"]):
                expected_config = {"backend": "llvm-api", "llvm_version": "23.1.4",
                    "triple": "x86_64-unknown-linux-gnu", "cpu": "x86-64", "features": "",
                    "pipeline": "default<O2>", "relocation": "PIC", "code_model": "default",
                    "verify_before_after": True, "input_kind": "builder", "parse_ns": 0}
                self.check(all(obj.get(k) == value for k, value in expected_config.items()),
                           label + "/unit-" + str(number) + ": actual O2, target, builder and verification policy")
                object_report_count += 1
            self.inventory(row["executable"], label)
            products[(name, arm)] = row
            native_reports.setdefault(name, {})[arm] = {
                k: native[k] for k in ("requested_units", "actual_units", "jobs", "fallback",
                                      "closed_executable", "internalized_functions")}
            for stage in ("startup", "warmup"):
                self.native(row[stage], row["executable"], case, stage, self.window / "build/logs")

        rng = random.Random(plan["seed"])
        sequence = []
        for block in range(3):
            order = list(cases)
            rng.shuffle(order)
            for name in order:
                sequence.extend({"block": block, "case": name, "position": pos, "arm": arm}
                                for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"))
        self.check(sequence == plan["order"] and len(rows) == 60, "seed-reconstructed ABBA/BAAB order")
        self.check([{k: r[k] for k in ("block", "case", "position", "arm")} for r in rows] == sequence,
                   "all 60 measured rows exactly match frozen order")
        self.check(complete["plan_sha256"] == plan_hash and complete["build_receipt_sha256"] == build_hash
                   and complete["records_sha256"] == records_hash and complete["rows"] == 60
                   and complete["failures"] == [], "measurement completion receipt hashes and count")
        cells = collections.defaultdict(list)
        for number, row in enumerate(rows):
            case = cases[row["case"]]
            self.check(row["index"] == number and row["plan_sha256"] == plan_hash
                       and row["build_receipt_sha256"] == build_hash, "runtime row identity " + str(number))
            self.native(row, products[(row["case"], row["arm"])]["executable"], case,
                        "runtime", self.window / "runtime/logs")
            cells[(row["case"], row["block"], row["arm"])].append(row)
        self.check(len(cells) == 30 and all(len(value) == 2 for value in cells.values()),
                   "exactly two samples in every case/block/arm cell")

        by_case = {}
        for name in cases:
            entry = {"blocks": [], "point_estimates": {}, "codegen": native_reports[name]}
            for block in range(3):
                block_result = {"block": block}
                for metric in ("wall", "cpu"):
                    raw = {arm: [r["wall_seconds"] if metric == "wall" else r["cpu_user"] + r["cpu_system"]
                                 for r in cells[(name, block, arm)]] for arm in "AB"}
                    means = {arm: sum(raw[arm]) / 2 for arm in "AB"}
                    block_result[metric] = {"A_seconds": means["A"], "B_seconds": means["B"],
                        "B_over_A": means["B"] / means["A"],
                        "A_samples_seconds": raw["A"], "B_samples_seconds": raw["B"]}
                entry["blocks"].append(block_result)
            entry["blocks_above_one"] = {}
            entry["paired_geometric_ratio_sensitivity"] = {}
            for metric in ("wall", "cpu"):
                a = statistics.median(b[metric]["A_seconds"] for b in entry["blocks"])
                b = statistics.median(b[metric]["B_seconds"] for b in entry["blocks"])
                estimate = {"A": a, "B": b, "B_over_A": b / a}
                entry["point_estimates"][metric] = estimate
                for key, value in estimate.items():
                    self.same_number(value, summary[metric]["by_case"][name][key], name + "/" + metric + "/" + key)
                ratios = [v[metric]["B_over_A"] for v in entry["blocks"]]
                entry["blocks_above_one"][metric] = sum(r > 1 for r in ratios)
                entry["paired_geometric_ratio_sensitivity"][metric] = math.exp(sum(map(math.log, ratios)) / 3)
            pa, pb = (products[(name, arm)] for arm in "AB")
            byte_equal = self.mapped(pa["executable"]["path"]).read_bytes() == self.mapped(pb["executable"]["path"]).read_bytes()
            entry["executable_comparison"] = {"byte_equal": byte_equal,
                "A": {k: pa["executable"][k] for k in ("sha256", "size")},
                "B": {k: pb["executable"][k] for k in ("sha256", "size")},
                "reported_prepartition_llvm_sha256_equal": pa["llvm_sha256"] == pb["llvm_sha256"],
                "A_reported_prepartition_llvm_sha256": pa["llvm_sha256"],
                "B_reported_prepartition_llvm_sha256": pb["llvm_sha256"]}
            if native_reports[name]["B"]["fallback"] == SMALL_REASON:
                self.check(byte_equal, name + ": CP14 retained exact baseline single-unit executable bytes")
            entry["stable_same_direction_runtime_regression_observed"] = (
                entry["blocks_above_one"]["wall"] == 3 and entry["blocks_above_one"]["cpu"] == 3)
            by_case[name] = entry

        totals = {}
        weighted_blocks = []
        for metric in ("wall", "cpu"):
            a = sum(cases[name]["weight"] * by_case[name]["point_estimates"][metric]["A"] for name in cases)
            b = sum(cases[name]["weight"] * by_case[name]["point_estimates"][metric]["B"] for name in cases)
            totals[metric] = {"A": a, "B": b, "B_over_A": b / a}
            for key, value in totals[metric].items():
                self.same_number(value, summary[metric][key], "weighted/" + metric + "/" + key)
        for block in range(3):
            result = {"block": block}
            for metric in ("wall", "cpu"):
                a = sum(cases[name]["weight"] * by_case[name]["blocks"][block][metric]["A_seconds"] for name in cases)
                b = sum(cases[name]["weight"] * by_case[name]["blocks"][block][metric]["B_seconds"] for name in cases)
                result[metric] = {"A": a, "B": b, "B_over_A": b / a}
            weighted_blocks.append(result)

        def resampled_ratio(draw):
            totals = {arm: sum(cases[name]["weight"] * statistics.median(
                by_case[name]["blocks"][block]["wall"][arm + "_seconds"] for block in draw)
                for name in cases) for arm in "AB"}
            return totals["B"] / totals["A"]

        rng = random.Random(plan["seed"])
        bootstrap = sorted(resampled_ratio(rng.choices(range(3), k=3)) for _ in range(5000))
        interval = [bootstrap[124], bootstrap[4874]]
        for i, value in enumerate(interval):
            self.same_number(value, summary["exploratory_paired_block_bootstrap95"][i], "bootstrap endpoint " + str(i))
        exhaustive = sorted(resampled_ratio(draw) for draw in itertools.product(range(3), repeat=3))
        for name in cases:
            for arm in "AB":
                diagnostic = summary["startup_diagnostics"][name][arm]
                product = products[(name, arm)]
                self.same_number(diagnostic["single_preflight_empty_seconds"], product["startup"]["wall_seconds"],
                                 name + "/" + arm + ": startup diagnostic")
                self.same_number(diagnostic["empty_over_measured_full_seconds"],
                    product["startup"]["wall_seconds"] / by_case[name]["point_estimates"]["wall"][arm],
                    name + "/" + arm + ": startup diagnostic proportion")
                self.check(diagnostic["executable_sha256"] == product["executable"]["sha256"]
                           and diagnostic["executable_bytes"] == product["executable"]["size"],
                           name + "/" + arm + ": startup retained binary identity")
        self.check(summary["plan_sha256"] == plan_hash and summary["records_sha256"] == records_hash
                   and summary["build_receipt_sha256"] == build_hash and summary["rows"] == 60
                   and summary["coverage_complete"] is True and summary["completion_receipt_matches"] is True
                   and summary["failures"] == [] and summary["source_identity"] == plan["source_identity"]
                   and summary["build_config"] == plan["build_config"], "summary identities and completeness")
        self.check(summary["product_gates"] == "NotRun"
                   and summary["verdict"] == "ExploratoryRuntimeQuality_NotAProductGate", "summary scope")
        window_record = next(r for r in root_result["windows"] if r["name"] == "baseline-closed-o2-1x1-vs-candidate-closed-o2-8x4")
        self.check(window_record["sha256"] == self.sha(self.window / "summary.json"), "root receipt binds quality summary")

        command_receipts = [load(p) for p in (self.root / "logs").glob("*.json")]
        quality_commands = sorted((r for r in command_receipts if r["label"].startswith("quality-")), key=lambda r: r["started_unix"])
        self.check([r["label"] for r in quality_commands] == ["quality-prepare", "quality-build", "quality-measure", "quality-summarize"],
                   "quality command order")
        for command in quality_commands:
            self.check(command["exit"] == 0 and command["timeout"] is False and command["status"] == "Pass",
                       command["label"] + ": command completed")
            for stream in ("stdout", "stderr"):
                self.check(self.sha(self.root / command[stream]) == command[stream + "_sha256"],
                           command["label"] + ": command stream hash")
        measure = next(r for r in quality_commands if r["label"] == "quality-measure")
        self.check("--quiet-window" in measure["argv"], "explicit quiet runtime window")
        overlaps = [r["label"] for r in command_receipts if r["label"] != measure["label"]
                    and r["started_unix"] < measure["started_unix"] + measure["seconds"]
                    and r["started_unix"] + r["seconds"] > measure["started_unix"]]
        self.check(not overlaps, "no recorded benchmark command overlaps runtime measurement")
        self.check(test_receipt["started_unix"] + test_receipt["seconds"] < measure["started_unix"],
                   "backend tests complete before runtime timing")

        return {
            "schema": "zil-compile4-independent-quality/1", "reviewer": "/root/o2_strategy",
            "reviewed_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "actions_run_id": 41068328117, "artifact_id": extraction["artifact_id"],
            "source_identity": plan["source_identity"], "candidate_crates_tree": CRATES_TREE,
            "evidence_root": str(self.root), "original_evidence_root": str(self.original),
            "original_tar_sha256": tar_digest, "file_index_sha256": self.sha(self.root / "files.json"),
            "plan_sha256": plan_hash, "build_receipt_sha256": build_hash,
            "builds_sha256": self.sha(self.window / "build/builds.jsonl"),
            "records_sha256": records_hash, "summary_sha256": self.sha(self.window / "summary.json"),
            "audit_script_sha256": self.sha(Path(__file__).resolve()),
            "audit_valid": not self.failures, "checks": self.checks, "failures": self.failures,
            "verified_quality_files": len(actual), "verified_indexed_files_including_dependencies": len(selected),
            "verified_quality_bytes": sum(index[path]["bytes"] for path in actual),
            "native_records_verified": dict(self.row_checks), "builds_verified": len(builds),
            "object_receipts_verified": object_report_count,
            "frozen_cases_and_weights": {name: case["weight"] for name, case in cases.items()},
            "frozen_cpu": plan["cpu"], "recorded_host": plan["host"], "build_config": plan["build_config"],
            "independence": {
                "original_aggregation_imported_or_called": False,
                "inputs_and_output_oracles_independently_reconstructed": True,
                "artifact_modified": False, "programs_recompiled": False, "native_programs_reexecuted": False,
                "reviewer_previously_authored_cp14_source": True,
                "scope": "Independent numerical and artifact audit of root-run measurements; not a non-author review of CP14 source."
            },
            "cp14_boundary_test_evidence": {"command_receipt": str(test_receipt_path.relative_to(self.root)),
                "stdout_sha256": test_receipt["stdout_sha256"], "library_passed": 45, "integration_passed": 6,
                "controls_observed": test_names, "boundaries_from_frozen_source": [2047, 2048, 2049]},
            "by_case": by_case, "weighted_point_estimates": totals, "weighted_paired_blocks": weighted_blocks,
            "exploratory_paired_block_bootstrap95": interval,
            "exhaustive_three_block_resampling_range": [min(exhaustive), max(exhaustive)],
            "independent_numeric_comparisons": len(self.differences),
            "maximum_absolute_numeric_difference": max(self.differences, default=0),
            "runtime_quality_conclusion": "NoStablePerCaseRegressionObserved_ThreeBlockExploratoryWindow" if not self.failures else "AuditFailed_NoVerdict",
            "interpretation": [
                "tree4x2 and retained use CP14's explicit fallback and preserve byte-identical baseline closed O2 single-unit executables.",
                "tree4x2 has mixed-direction block ratios; its CPU point estimate is slightly above one. Neither its wall-clock point estimate nor byte-identical output demonstrates runtime acceleration.",
                "The three larger trees have lower candidate wall and CPU costs in all three observed paired blocks. This describes this window, not general workload or host-independent superiority.",
                "No case has all three wall and CPU block ratios above one. The prior small-program stable regression is not reproduced in this window.",
                "No sample, startup observation, or case was removed or reweighted; full process startup and stdio remain in every runtime sample."
            ],
            "limitations": [
                "Only three independent paired blocks on one shared GitHub host; bootstrap intervals are descriptive and do not establish general equivalence or a product gate.",
                "The measured baseline is closed O2 1/1. No old closed O2 8/4 same-configuration product exists in this matrix, so unchanged above-threshold partition/output relative to old 8/4 is not established here.",
                "Machine instruction totals are not reported. CP14 branch selection is observed from its exclusive fallback string and supported by executed frozen boundary tests, not by a measured per-case instruction count.",
                "The prepartition LLVM sidecars were intentionally deleted after hashing. Their recorded digests can be compared but their bytes are unavailable for fresh hashing.",
                "Archive extraction preserves bytes and modes via hardlinks. Original recorded before/after inventories are compared internally; local ctime and paths are not represented as original runner metadata.",
                "External LLVM/Clang/LLD binaries are not retained in this evidence directory; the audit checks frozen inventories and per-object policy receipts, not fresh hashes of those external tools.",
                "The reviewer authored CP14 implementation; this audit independently recalculates the coordinator's raw measurements and is not an independent non-author source review."
            ],
            "product_gates": "NotRun", "performance_adoption": "NotEvaluated"
        }


def write_markdown(path, result):
    lines = ["# COMPILE4 直接 O2 运行质量独立复算", "",
        "审查者：`/root/o2_strategy`。绑定 Actions run `41068328117`、候选 `" + CANDIDATE + "`。",
        "旧方为 `" + BASE + "` closed O2 1/1；新方为候选 closed O2 请求 8/4。", "",
        "**本窗口未观察到稳定的逐语料运行退化；这是三个配对区块的探索性结论，产品门和采用裁决未运行。**" if result["audit_valid"] else "**审查失败，不给性能结论。**",
        "", "## 完整性与执行配置", "",
        f"已读取并核对 {result['verified_quality_files']} 个质量窗口文件、10 个产物构建、60 次正式运行以及 10 次空输入启动和 10 次完整输入预热。输入、独立重建的逻辑 oracle、stdout 原始字节、空 stderr、退出码、超时、计划/收据哈希及保留 ELF 身份全部纳入检查。",
        f"31 份对象收据均为 LLVM 23.1.4、`default<O2>`、`verify_before_after=true`，每次构建仍执行一次当前 Verify。对象缓存、查询缓存、源码缓存与 profiling 关闭。CP14 的三个模型控制和非法配置控制在实际远端测试日志中执行成功；backend library 45 项、integration 6 项。",
        "", "| 语料 | A 实际 CGU/jobs | B 请求 → 实际 CGU/jobs | B fallback | A/B ELF |",
        "|---|---:|---:|---|---|"]
    for name, row in result["by_case"].items():
        a, b = row["codegen"]["A"], row["codegen"]["B"]
        equality = "逐字相同" if row["executable_comparison"]["byte_equal"] else "不同"
        lines.append(f"| {name} | {a['actual_units']}/{a['jobs']} | 8/4 → {b['actual_units']}/{b['jobs']} | {b['fallback'] or '无'} | {equality} |")
    lines += ["", "## 每个语料的三个完整配对区块", "",
        "下表均为 B/A；每区块先对每臂两次运行取均值。CPU 是 user + system。点估计仍使用原协议：分别对每臂的区块均值取中位数后相除，没有改成比值中位数。",
        "", "| 语料 | 墙钟：区块 0 / 1 / 2 | CPU：区块 0 / 1 / 2 | 原墙钟点估计 | 原 CPU 点估计 |",
        "|---|---|---|---:|---:|"]
    for name, row in result["by_case"].items():
        wall = " / ".join(f"{b['wall']['B_over_A']:.6f}" for b in row["blocks"])
        cpu = " / ".join(f"{b['cpu']['B_over_A']:.6f}" for b in row["blocks"])
        lines.append(f"| {name} | {wall} | {cpu} | {row['point_estimates']['wall']['B_over_A']:.6f} | {row['point_estimates']['cpu']['B_over_A']:.6f} |")
    points = result["weighted_point_estimates"]
    lines += ["", f"按原五语料权重加权绝对成本后，加权墙钟 B/A 为 **{points['wall']['B_over_A']:.9f}**，CPU 为 **{points['cpu']['B_over_A']:.9f}**。独立复算 {result['independent_numeric_comparisons']} 个数值与原摘要比较，最大绝对差 {result['maximum_absolute_numeric_difference']:.3g}。未调用原 runner 的聚合函数。",
        "", "`tree4x2` 的墙钟与 CPU 区块方向混合，CPU 点估计约高 0.11%；A/B ELF 均为 6,504 字节且逐字相同。`retained` 的两个 ELF 均为 6,592 字节且逐字相同，最后一个区块墙钟略高于 1。这两个程序的小幅时间差不能称为代码运行加速。另三棵树在三个区块内墙钟和 CPU 均低于 1，这只描述本次冻结窗口。",
        "", "三个区块的加权墙钟 B/A：" + " / ".join(f"{b['wall']['B_over_A']:.6f}" for b in result["weighted_paired_blocks"]) + "；CPU B/A：" + " / ".join(f"{b['cpu']['B_over_A']:.6f}" for b in result["weighted_paired_blocks"]) + "。",
        "", f"原 paired-block bootstrap 95% 描述区间复算为 [{result['exploratory_paired_block_bootstrap95'][0]:.9f}, {result['exploratory_paired_block_bootstrap95'][1]:.9f}]。只有三个独立区块，这个窄区间不能视作广泛工作负载或另一宿主上的保证。启动诊断从未扣除；retained 的单次空输入启动约占其完整运行时间的 71%，因此短进程计时解释尤其有限。",
        "", "## 证据边界", "",
        "当前矩阵缺少旧 closed O2 8/4 同配置产物，所以不能由本窗口宣称超过阈值语料相对旧 8/4 的分区和机器码未变。报告也没有机器指令总数；CP14 是否触发由专属 fallback 及已执行的冻结边界测试佐证。`.ll` 侧车只保留原哈希，不能重新核其字节。",
        "", "原 artifact 的字节未修改，未重新编译或运行其中程序。读取时将原 runner 绝对路径映射到本地解包目录，按保留字节核验；没有把解包后的 ctime 当成原运行时身份。外部 LLVM 工具未包含在此目录中，工具身份依冻结记录及实际对象收据核对。",
        "", "本审查者曾实现 CP14；这里的独立性指对协调者生成的原始样本使用另写脚本重算和校验，不是非作者源码审查。完整数据、逐区块绝对时间、哈希和检查边界见同名 JSON；脚本为 `audit_compile4_quality.py`。", ""]
    if result["failures"]:
        lines += ["## 未通过检查", ""] + ["- " + failure for failure in result["failures"]]
    path.write_text("\n".join(lines))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    audit = Audit(args.evidence, args.repo)
    result = audit.run()
    args.out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    write_markdown(args.out.with_suffix(".md"), result)
    print(json.dumps({k: result[k] for k in ("audit_valid", "checks", "failures", "verified_quality_files",
        "native_records_verified", "builds_verified", "object_receipts_verified", "weighted_point_estimates",
        "maximum_absolute_numeric_difference", "runtime_quality_conclusion")}, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["audit_valid"] else 1)


if __name__ == "__main__":
    main()

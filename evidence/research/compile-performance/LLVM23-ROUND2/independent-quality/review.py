#!/usr/bin/env python3
"""Read-only independent review of the frozen LLVM23 round-two O2 quality data.

Only this review's requested JSON output is written. No compiler, native sample,
benchmark runner, or author aggregation function is imported or executed.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import sys

sys.dont_write_bytecode = True
CASES = ("tree4x2", "tree32x8", "tree256x8", "tree1024x8", "retained")
WEIGHTS = (.05, .15, .30, .30, .20)
COUNTS = {"closed8": (9, 261, 2081, 8321, 9),
          "closed1": (16, 292, 2308, 9220, 11)}
UNITS = {"closed8": (7, 8, 8, 8, 3), "closed1": (1, 1, 1, 1, 1)}
JOBS = {"closed8": (4, 4, 4, 4, 3), "closed1": (1, 1, 1, 1, 1)}
RUNNER_COMMIT = "0c6fe73ef9cacfe9ca080e4c35d109d38b50c462"
BASELINE_COMMIT = "e8e274c1b28509547a4854d46708ae1601e756e4"
CANDIDATE_COMMIT = "6e8e63a5ceae861447d0447c325bed78b59c3f42"


class ReviewFailure(Exception):
    pass


def require(condition, label):
    if not condition:
        raise ReviewFailure(label)


def read_json(path):
    return json.loads(Path(path).read_text())


def read_rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


class Reviewer:
    def __init__(self, root, repo):
        self.root, self.repo = root, repo
        self.checked = {}
        self.hash_cache = {}

    def sha(self, path):
        path = Path(path)
        before = path.stat()
        key = (str(path.resolve()), before.st_size, before.st_mtime_ns, before.st_ctime_ns)
        if key not in self.hash_cache:
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    digest.update(chunk)
            after = path.stat()
            require((after.st_size, after.st_mtime_ns, after.st_ctime_ns) == key[1:],
                    "file changed during hash: " + str(path))
            self.hash_cache[key] = digest.hexdigest()
        self.checked[str(path)] = self.hash_cache[key]
        return self.hash_cache[key]

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args])

    def identity(self, frozen, native=False):
        path = Path(frozen["path"])
        stat = path.stat()
        for name, actual in (("size", stat.st_size), ("mtime_ns", stat.st_mtime_ns),
                             ("ctime_ns", stat.st_ctime_ns)):
            require(frozen[name] == actual, "identity " + name + ": " + str(path))
        require(self.sha(path) == frozen["sha256"], "identity sha: " + str(path))
        if native:
            with path.open("rb") as stream:
                header = stream.read(20)
            require(header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\x3e\x00",
                    "expected native ELF64 x86-64 executable: " + str(path))

    def fixture_set(self, root, frozen):
        actual = {str(p.relative_to(root)): self.sha(p)
                  for prefix in ("sources", "inputs", "expected")
                  for p in (root / prefix).rglob("*") if p.is_file()}
        require(actual == frozen, "complete frozen source/input/oracle set: " + str(root))

    def native_check(self, row, plan_root, logs, executable, input_name, expected_name):
        require(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False,
                "native success/exit/timeout")
        require(row["executable"] == row["executable_after"] == executable,
                "native executable before/after/build binding")
        require(row["input"] == input_name and row["expected"] == expected_name,
                "native input/oracle path binding")
        data = (plan_root / input_name).read_bytes()
        expected = (plan_root / expected_name).read_bytes()
        require(sha_bytes(data) == row["input_sha256"] == row["input_after_sha256"],
                "native input hash binding")
        require(sha_bytes(expected) == row["expected_stdout_sha256"] == row["stdout_sha256"],
                "native oracle hash binding")
        for name in ("stdout", "stderr"):
            p = logs / row[name]
            require(p.parent.resolve() == logs.resolve(), "unexpected log path")
            require(self.sha(p) == row[name + "_sha256"], "native raw " + name + " hash")
        require((logs / row["stdout"]).read_bytes() == expected, "native stdout oracle bytes")
        require((logs / row["stderr"]).stat().st_size == 0, "native stderr must be empty")
        require(row["stdout_bytes"] == len(expected), "native stdout length")
        for name in ("wall_seconds", "cpu_user", "cpu_system"):
            v = row[name]
            require(type(v) in (int, float) and math.isfinite(v) and v >= 0,
                    "finite nonnegative native metric: " + name)
        require(row["wall_seconds"] > 0 and row["cpu_user"] + row["cpu_system"] > 0,
                "positive native wall and CPU")

    def native_report(self, raw, case, arm, mode):
        require(raw["status"] == "Success" and raw["phase"] == "link"
                and raw["diagnostics"] == [] and raw["timings"] is None,
                "raw build success/link/diagnostics/timings")
        require(type(raw["verification_runs"]) is int and raw["verification_runs"] == 1,
                "raw build exactly one independent Verify")
        require(raw["cache_hit"] is False and raw["disk_cache"] == "disabled",
                "raw source cache disabled")
        n = raw["native_codegen"]
        require(n["backend"] == "llvm-api" and n["fallback"] is None,
                "native API and no fallback")
        i = CASES.index(case)
        if arm == "B":
            require(n["closed_executable"] is True
                    and type(n["internalized_functions"]) is int
                    and n["internalized_functions"] == COUNTS[mode][i],
                    "explicit closed mode and expected linkage change count")
            require(n["requested_units"] == (8 if mode == "closed8" else 1)
                    and n["actual_units"] == UNITS[mode][i] and n["jobs"] == JOBS[mode][i],
                    "reported CGU/jobs")
            require(raw["query_cache"]["requested"] is False
                    and raw["query_cache"]["state"] == "disabled", "query cache disabled")
        else:
            require(n.get("closed_executable", False) is False
                    and n["requested_units"] == n["actual_units"] == n["jobs"] == 1,
                    "baseline O2 single CGU, closed disabled")
        oc = n["object_cache"]
        require(oc["requested"] is False and oc["enabled"] is False
                and all(oc[k] == 0 for k in ("hits", "misses", "corrupt", "stored", "write_failures")),
                "machine object cache disabled")
        require(len(n["object_reports"]) == n["actual_units"], "one object report per unit")
        for obj in n["object_reports"]:
            require(obj["backend"] == "llvm-api" and obj["llvm_version"] == "23.1.4"
                    and obj["pipeline"] == "default<O2>" and obj["verify_before_after"] is True
                    and obj["input_kind"] == "builder" and obj["triple"] == "x86_64-unknown-linux-gnu"
                    and obj["cpu"] == "x86-64" and obj["features"] == "",
                    "LLVM version/pipeline/verification/target/Builder")
        require(len(raw["commands"]) == 1, "one native link command")
        link = raw["commands"][0]
        require("--target=x86_64-unknown-linux-gnu" in link and "-O2" in link
                and any(x.endswith("/bin/ld.lld") and x.startswith("--ld-path=") for x in link),
                "paired LLVM LLD link command")
        return {"case": case, "arm": arm, "closed_executable": n.get("closed_executable", False),
                "internalized_functions": n.get("internalized_functions", 0),
                "requested_units": n["requested_units"], "actual_units": n["actual_units"],
                "jobs": n["jobs"], "verification_runs": 1, "object_reports": len(n["object_reports"]),
                "object_bytes": sum(o["object_bytes"] for o in n["object_reports"]),
                "pipeline": "default<O2>", "fallback": None}

    def quality(self, mode):
        plan_root = self.root / ("quality-o2-" + mode + "-plan")
        build_root = self.root / ("quality-o2-" + mode + "-build")
        measure = self.root / "measurements-confirmation"
        runtime = measure / ("quality-o2-" + mode + "-runtime")
        plan_path, receipt_path = plan_root / "plan.json", build_root / "result.json"
        plan, receipt = read_json(plan_path), read_json(receipt_path)
        plan_hash, receipt_hash = self.sha(plan_path), self.sha(receipt_path)
        require(plan["schema"] == "zil-cperf-native-runtime/1" and plan["blocks"] == 5
                and plan["expected_rows"] == 100 and plan["cpu"] == 0, "frozen runtime protocol")
        require(tuple(c["id"] for c in plan["cases"]) == CASES
                and tuple(c["weight"] for c in plan["cases"]) == WEIGHTS, "fixed five-case weights")
        order_rng = random.Random(plan["seed"])
        expected_order = []
        for block in range(5):
            shuffled = list(CASES)
            order_rng.shuffle(shuffled)
            for case in shuffled:
                expected_order.extend({"block": block, "case": case, "position": pos, "arm": arm}
                                      for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"))
        require(plan["order"] == expected_order, "seeded case order and ABBA/BAAB protocol")
        require(plan["profile"] == "O2" and plan["object_cache"] is False and plan["timings"] is False,
                "frozen O2/no-cache/no-timings mode")
        expected_config = {"A": {"profile": "O2", "units": 1, "jobs": 1, "closed_executable": False},
                           "B": {"profile": "O2", "units": 8 if mode == "closed8" else 1,
                                 "jobs": 4 if mode == "closed8" else 1, "closed_executable": True}}
        require(plan["build_config"] == expected_config, "baseline/candidate frozen build mode")
        require(plan["runner_source_commit"] == RUNNER_COMMIT, "runtime runner exact commit")
        runner = "tools/research/compile-performance/runtime_bench.py"
        require(sha_bytes(self.git("show", RUNNER_COMMIT + ":" + runner)) == plan["runner_sha256"],
                "runner bytes at frozen Git commit")
        for name, digest in plan["helper_sha256"].items():
            require(sha_bytes(self.git("show", RUNNER_COMMIT + ":" + name)) == digest,
                    "helper bytes at frozen commit: " + name)
        for arm, commit in (("A", BASELINE_COMMIT), ("B", CANDIDATE_COMMIT)):
            identity = plan["source_identity"][arm]
            require(identity["commit"] == commit
                    and self.git("rev-parse", commit + ":crates").decode().strip() == identity["crates_tree"],
                    "compiler declared source/tree identity")
        for identity in [*plan["binaries"].values(), *plan["extra_tools"]]:
            self.identity(identity)
        upstream = plan["upstream_corpus_plan"]
        require(self.sha(upstream["path"]) == upstream["sha256"], "upstream corpus plan binding")
        self.fixture_set(plan_root, plan["fixtures_sha256"])
        require(receipt["plan_sha256"] == plan_hash and receipt["pass"] is True
                and receipt["failures"] == [] and receipt["rows"] == 10
                and receipt["runtime_samples"] == 0
                and receipt["builds_sha256"] == self.sha(build_root / "builds.jsonl"),
                "complete unscored native-build receipt")
        builds = read_rows(build_root / "builds.jsonl")
        require([(r["case"], r["arm"]) for r in builds] == [(c, a) for c in CASES for a in "AB"],
                "complete build cases and arms in fixed order")
        cases = {c["id"]: c for c in plan["cases"]}
        artifacts, native = {}, []
        for row in builds:
            case, arm = cases[row["case"]], row["arm"]
            source_prefix = case["source"] + "/"
            require({name[len(source_prefix):]: digest for name, digest in plan["fixtures_sha256"].items()
                     if name.startswith(source_prefix)} == case["source_sha256"],
                    "case source receipt binds actual frozen source files")
            require(row["pass"] is True and row["plan_sha256"] == plan_hash, "successful bound build")
            require(row["source_before"] == row["source_after"] == case["source_sha256"],
                    "source hashes unchanged across native build")
            config = expected_config[arm]
            executable = row["executable"]
            expected_argv = [plan["binaries"][arm]["path"], "build", str(plan_root / case["source"] / "main.zil"),
                             "-o", executable["path"], "--json", "--backend", "llvm-api", "--opt", "O2",
                             "--codegen-units", str(config["units"]), "--codegen-jobs", str(config["jobs"]),
                             "--clang", plan["binaries"]["clang"]["path"]]
            if config["closed_executable"]:
                expected_argv.append("--closed-executable")
            proc = row["build_process"]
            require(row["argv"] == proc["argv"] == expected_argv and proc["exit"] == 0
                    and proc["timeout"] is False, "actual successful compiler command binding")
            for stream in ("stdout", "stderr"):
                require(self.sha(build_root / "logs" / row[stream]) == row[stream + "_sha256"],
                        "raw compiler output binding")
            require((build_root / "logs" / row["stderr"]).stat().st_size == 0, "compiler stderr")
            raw = read_json(build_root / "logs" / row["stdout"])
            require(raw["artifacts"]["executable"] == executable["path"], "raw native artifact binding")
            report = self.native_report(raw, case["id"], arm, mode)
            require(row["compiler_status"] == raw["status"] and type(row["verification_runs"]) is int
                    and row["verification_runs"] == 1 and row["timings_disabled"] is True,
                    "build receipt matches full raw report")
            self.identity(executable, native=True)
            report["executable"] = executable
            native.append(report)
            self.native_check(row["startup"], plan_root, build_root / "logs", executable,
                              "inputs/empty.bin", "expected/empty.bin")
            self.native_check(row["warmup"], plan_root, build_root / "logs", executable,
                              case["input"], case["expected"])
            artifacts[case["id"], arm] = executable
        complete = read_json(runtime / "complete.json")
        records_path = runtime / "records.jsonl"
        require(complete["plan_sha256"] == plan_hash and complete["build_receipt_sha256"] == receipt_hash
                and complete["rows"] == 100 and complete["failures"] == []
                and complete["records_sha256"] == self.sha(records_path), "complete runtime receipt")
        rows = read_rows(records_path)
        require(len(rows) == len(plan["order"]) == 100, "full runtime row count")
        for i, (row, order) in enumerate(zip(rows, plan["order"])):
            require(row["index"] == i and all(row[k] == order[k] for k in ("block", "case", "position", "arm"))
                    and row["plan_sha256"] == plan_hash and row["build_receipt_sha256"] == receipt_hash,
                    "runtime frozen ABBA/BAAB order and identity")
            case = cases[row["case"]]
            self.native_check(row, plan_root, runtime / "logs", artifacts[row["case"], row["arm"]],
                              case["input"], case["expected"])
        result = independent_statistics(rows, plan["seed"])
        # The author summary is consulted only after all independent values exist.
        summary_path = measure / ("quality-o2-" + mode + "-runtime-summary.json")
        summary = read_json(summary_path)
        require(summary["plan_sha256"] == plan_hash and summary["build_receipt_sha256"] == receipt_hash
                and summary["records_sha256"] == self.sha(records_path) and summary["rows"] == 100
                and summary["coverage_complete"] is True and summary["completion_receipt_matches"] is True
                and summary["failures"] == [] and summary["source_identity"] == plan["source_identity"]
                and summary["build_config"] == expected_config, "author summary original input bindings")
        for metric in ("wall", "cpu"):
            for key in ("A", "B", "B_over_A"):
                require(math.isclose(result[metric][key], summary[metric][key], rel_tol=1e-12, abs_tol=1e-15),
                        "independent weighted value differs from author: " + metric + "/" + key)
                for case in CASES:
                    require(math.isclose(result[metric]["by_case"][case][key], summary[metric]["by_case"][case][key],
                                         rel_tol=1e-12, abs_tol=1e-15), "independent per-case value differs from author")
        require(all(math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-15)
                    for a, b in zip(result["wall"]["paired_block_bootstrap95"],
                                    summary["exploratory_paired_block_bootstrap95"])), "independent weighted wall CI")
        result.update({"mode": mode, "integrity_verdict": "Pass", "runtime_rows": 100,
                       "native_builds": 10, "unscored_startup_checks": 10, "unscored_full_warmup_checks": 10,
                       "source_identity": plan["source_identity"], "build_config": expected_config,
                       "native_build_reports": native, "author_summary_reproduced": True,
                       "bindings": {str(path): self.sha(path) for path in
                                    (plan_path, receipt_path, build_root / "builds.jsonl", records_path,
                                     runtime / "complete.json", summary_path)}})
        return result

    def prior_preflight(self, mode):
        """Persist the earlier read-only Rust/ZIL preflight check; no performance score."""
        plan_root = self.root / ("rust-o2-cold-" + mode + "-plan")
        preflight = self.root / ("rust-o2-cold-" + mode + "-preflight")
        plan = read_json(plan_root / "plan.json")
        result = read_json(preflight / "result.json")
        require(plan["source_identity"]["B"]["commit"] == CANDIDATE_COMMIT
                and plan["schema"] == "zil-rust-compile-performance/2"
                and plan["profile"] == "O2" and plan["cache_mode"] == "cold"
                and plan["query_cache"] is False and plan["closed_executable"] is True,
                "prior preflight frozen mode")
        require(result["pass"] is True and result["failures"] == [] and result["rows"] == 10,
                "prior preflight complete success")
        for key, path in (("plan", plan_root / "plan.json"), ("records", preflight / "records.jsonl"),
                          ("artifacts", preflight / "artifacts.json")):
            require(self.sha(path) == result[key + "_sha256"], "prior preflight binding: " + key)
        rows = read_rows(preflight / "records.jsonl")
        artifacts = read_json(preflight / "artifacts.json")
        amap = {(a["case"], a["arm"]): a for a in artifacts}
        require(len(rows) == len(artifacts) == len(amap) == 10, "prior preflight row coverage")
        reports, checks = [], 0
        for row in rows:
            require(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False
                    and row["sample_kind"] == "preflight-unscored"
                    and row["plan_sha256"] == result["plan_sha256"]
                    and row["source_before"] == row["source_after"], "prior build success/source preservation")
            for key in ("stdout", "stderr"):
                require(self.sha(preflight / "logs" / row[key]) == row[key + "_sha256"],
                        "prior raw build log binding")
            require(row["executable"]["sha256"] == amap[row["case"], row["arm"]]["executable"]["sha256"],
                    "prior build/preserved executable binding")
            for check in row["checks"]:
                self.native_check(check, plan_root, preflight / "logs", row["executable"],
                                  check["input"], check["expected"])
                checks += 1
            if row["arm"] == "B":
                reports.append(self.native_report(read_json(preflight / "logs" / row["stdout"]),
                                                  row["case"], "B", mode))
        for a in artifacts:
            self.identity(a["executable"], native=True)
            for key in ("startup", "warmup"):
                check = a[key]
                self.native_check(check, plan_root, preflight / "logs", a["executable"],
                                  check["input"], check["expected"])
        require(checks == 32 and len(reports) == 5, "prior 32 short oracles and five ZIL reports")
        return {"verdict": "Pass", "mode": mode, "sample_kind": "preflight-unscored",
                "plan_sha256": result["plan_sha256"], "records_sha256": result["records_sha256"],
                "artifacts_sha256": result["artifacts_sha256"], "short_oracles": checks,
                "startup_checks": 10, "full_warmup_checks": 10, "preserved_native_executables": 10,
                "actual_zil_reports": reports,
                "count_meaning": "Functions changed External to Internal before LLVM O2; not DCE counts",
                "scope": "Raw logs, input/oracle bytes, receipts and preserved executables rechecked; no runtime score"}


def independent_statistics(rows, seed):
    """Construct the 5x5x2 block table directly; never call the author aggregate."""
    cells = defaultdict(list)
    for row in rows:
        cells[row["case"], row["block"], row["arm"]].append(row)
    require(set(cells) == {(case, block, arm) for case in CASES for block in range(5) for arm in "AB"}
            and all(len(v) == 2 for v in cells.values()), "two samples per arm/case/block")
    rng = random.Random(seed)
    draws = [[int(rng.random() * 5) for _ in range(5)] for _ in range(5000)]
    answer = {"estimator": "Mean two same-arm samples within each paired block; median of five block means per arm/case; weighted sum of case medians; B/A",
              "interval_method": "5000 paired whole-block bootstrap draws with replacement, seed from plan; sorted ranks 124 and 4874 (zero based); same draws for wall/CPU/all cases",
              "bootstrap_seed": seed, "bootstrap_draws": 5000,
              "bootstrap_indices_sha256": sha_bytes(json.dumps(draws, separators=(",", ":")).encode()),
              "interval_scope": "Exploratory per-estimand intervals on five blocks, not simultaneous multiple-case coverage or proof of language-wide equivalence",
              "per_block": []}
    def interval(values):
        ordered = sorted(values)
        return [ordered[124], ordered[4874]]
    tables = {}
    for metric in ("wall", "cpu"):
        means = {}
        for (case, block, arm), group in cells.items():
            values = [r["wall_seconds"] if metric == "wall" else r["cpu_user"] + r["cpu_system"] for r in group]
            means[case, block, arm] = math.fsum(values) / 2
        tables[metric] = means
        samples = {case: [] for case in CASES}
        paired_log_samples = {case: [] for case in CASES}
        weighted_samples = []
        for selected in draws:
            totals = {"A": 0., "B": 0.}
            for case, weight in zip(CASES, WEIGHTS):
                medians = {arm: sorted(means[case, b, arm] for b in selected)[2] for arm in "AB"}
                samples[case].append(medians["B"] / medians["A"])
                for arm in "AB":
                    totals[arm] += weight * medians[arm]
                paired_log_samples[case].append(math.exp(math.fsum(
                    math.log(means[case, b, "B"] / means[case, b, "A"]) for b in selected) / 5))
            weighted_samples.append(totals["B"] / totals["A"])
        by_case, totals = {}, {"A": 0., "B": 0.}
        for case, weight in zip(CASES, WEIGHTS):
            medians = {arm: sorted(means[case, b, arm] for b in range(5))[2] for arm in "AB"}
            paired = [means[case, b, "B"] / means[case, b, "A"] for b in range(5)]
            ci = interval(samples[case])
            by_case[case] = {**medians, "B_over_A": medians["B"] / medians["A"],
                             "paired_block_bootstrap95": ci, "slower_blocks": sum(x > 1 for x in paired),
                             "block_ratios": paired, "stable_observed_regression": ci[0] > 1 and all(x > 1 for x in paired),
                             "paired_geometric_ratio_sensitivity": math.exp(math.fsum(math.log(x) for x in paired) / 5),
                             "paired_geometric_bootstrap95_sensitivity": interval(paired_log_samples[case])}
            for arm in "AB":
                totals[arm] += weight * medians[arm]
        answer[metric] = {**totals, "B_over_A": totals["B"] / totals["A"],
                          "paired_block_bootstrap95": interval(weighted_samples), "by_case": by_case}
    for block in range(5):
        out = {"block": block, "order": "ABBA" if block % 2 == 0 else "BAAB"}
        for metric, table in tables.items():
            per_case = {case: {"A": table[case, block, "A"], "B": table[case, block, "B"],
                               "B_over_A": table[case, block, "B"] / table[case, block, "A"]} for case in CASES}
            totals = {arm: math.fsum(weight * table[case, block, arm] for case, weight in zip(CASES, WEIGHTS))
                      for arm in "AB"}
            out[metric] = {**totals, "B_over_A": totals["B"] / totals["A"], "by_case": per_case}
        answer["per_block"].append(out)
    return answer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    require(not args.out.exists(), "review output must be new; never overwrite author files")
    reviewer = Reviewer(args.root.resolve(), args.repo.resolve())
    result = {"schema": "zil-independent-o2-quality-review/1", "reviewer": "/root/llvm_strategy",
              "implementation_commit": RUNNER_COMMIT, "candidate_compiler_commit": CANDIDATE_COMMIT,
              "baseline_compiler_commit": BASELINE_COMMIT,
              "review_scope": "Non-author independent runtime data/aggregation review; CP13 core implementation has separate non-author root review ac10f2c54",
              "script_sha256": reviewer.sha(Path(__file__).resolve()),
              "command": [sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]],
              "product_gates": "NotRun", "compiler_or_native_reexecution": False}
    identity_metadata = result.copy()
    try:
        quality = {mode: reviewer.quality(mode) for mode in ("closed8", "closed1")}
        preflight = {mode: reviewer.prior_preflight(mode) for mode in ("closed8", "closed1")}
        result.update(verdict="Pass", integrity="Pass", author_summaries_reproduced=True,
                      quality=quality, prior_real_preflight_review=preflight,
                      primary_o2_configuration_verdict="NoGo",
                      primary_o2_configuration_reason="closed8 tree4x2 slows in all five paired blocks; wall and CPU per-case intervals exclude 1 above it. Weighted near-one results do not cancel this observed regression.",
                      secondary_o2_configuration_verdict="ExploratoryOnly_NotPromoted",
                      secondary_o2_configuration_reason="All five per-case planned-estimator wall/CPU intervals cross 1. This finite shared-host window neither proves general equivalence nor removes observed slow blocks; secondary was not the preregistered primary.",
                      recommendation="Keep the closed-executable option default off; do not recommend the primary O2 configuration as a performance adoption or merge-to-main result.",
                      limitations=["Five paired blocks, fixed five-case workload, 80% synthetic workload weight, one shared host.",
                                   "Full native process includes startup and stdio; no startup subtraction or sample removal.",
                                   "Compiler source commits are declared by the frozen plans; binary hashes and real build logs are checked, not independently rebuilt here.",
                                   "Generated LLVM text sidecars were intentionally removed after hashing by the frozen build protocol; they are not rehashed by this review.",
                                   "Review Pass means data integrity and recomputation passed; it is distinct from the primary configuration's NoGo."])
        require(quality["closed8"]["wall"]["by_case"]["tree4x2"]["stable_observed_regression"]
                and quality["closed8"]["cpu"]["by_case"]["tree4x2"]["stable_observed_regression"],
                "documented primary NoGo must be supported by independent values")
        result["checked_file_count"] = len(reviewer.checked)
        result["checked_file_manifest_sha256"] = sha_bytes(json.dumps(reviewer.checked, sort_keys=True, separators=(",", ":")).encode())
    except (ReviewFailure, OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        result = {**identity_metadata, "verdict": "Fail", "integrity": "Fail",
                  "error": repr(error), "scores_suppressed": True}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("x") as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False, allow_nan=False)
        stream.write("\n")
    print(json.dumps({"verdict": result["verdict"], "output": str(args.out),
                      "primary_verdict": result.get("primary_o2_configuration_verdict"),
                      "error": result.get("error")}, ensure_ascii=False))
    return 0 if result["verdict"] == "Pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Independent, read-only audit of the four frozen Rust/ZIL round-two plans.

No benchmark module is imported and no compiler/program is executed. Decimal
arithmetic starts at JSON parsing. Original summaries are comparison targets,
never inputs to an estimate. The interrupted window is excluded explicitly.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from decimal import Decimal as D, getcontext, ROUND_CEILING
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import re
import shutil
import subprocess
import sys

getcontext().prec = 40
SOURCE = "6e8e63a5ceae861447d0447c325bed78b59c3f42"
CRATES = "d865b46fa776ab311734eebaa9c19eb0aa3b7b9d"
BINARY = "764f592fcee5a7e9780fd774b3d93f168ddf52ba2a2f7d11d125909e43eb46a3"
CASE_WEIGHTS = dict(zip(("tree4x2", "tree32x8", "tree256x8", "tree1024x8", "retained"),
                        map(D, (".05", ".15", ".30", ".30", ".20"))))
REQUEST_WEIGHTS = dict(zip(("cold_process", "unchanged", "private_edit", "public_edit"),
                           map(D, (".10", ".35", ".40", ".15"))))
SNAPSHOTS = dict(zip(REQUEST_WEIGHTS, ("base", "base", "private", "public")))
CONFIGS = {
    "rust-o0-cold": ("O0", "cold", 8, 4, False, False, "O0 primary comparison"),
    "rust-o0-incremental-query": ("O0", "incremental", 8, 4, True, False, "incremental primary comparison"),
    "rust-o2-cold-closed8": ("O2", "cold", 8, 4, False, True, "preregistered O2 primary confirmation"),
    "rust-o2-cold-closed1": ("O2", "cold", 1, 1, False, True, "O2 secondary exploratory; never promoted from this batch"),
}
EMPTY = hashlib.sha256(b"").hexdigest()
HEX = re.compile(r"^[0-9a-f]{64}$")


def decode(text):
    return json.loads(text, parse_float=D,
                      parse_constant=lambda s: (_ for _ in ()).throw(ValueError(s)))


def read(path):
    return decode(Path(path).read_text())


def records(path):
    return [decode(line) for line in Path(path).read_text().splitlines()]


def decimal_json(value):
    if isinstance(value, D):
        return str(value)
    raise TypeError(type(value).__name__)


class Audit:
    def __init__(self):
        self.counts = Counter()
        self.errors = []
        self.hashed = {}

    def check(self, ok, name, detail=""):
        self.counts[name] += 1
        if not ok:
            self.errors.append({"check": name, "detail": str(detail)})

    def sha(self, path):
        path = Path(path).absolute()
        before = path.stat()
        signature = (before.st_dev, before.st_ino, before.st_size,
                     before.st_mtime_ns, before.st_ctime_ns)
        if str(path) in self.hashed:
            previous = self.hashed[str(path)]
            self.check(previous[0] == signature, "file_stable_during_audit", path)
            return previous[1]
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        after = path.stat()
        self.check(signature == (after.st_dev, after.st_ino, after.st_size,
                                 after.st_mtime_ns, after.st_ctime_ns),
                   "file_stable_during_hash", path)
        self.hashed[str(path)] = (signature, digest.hexdigest())
        return digest.hexdigest()

    def inventory(self, frozen, aliases=False):
        path = Path(frozen["path"])
        stat = path.stat()
        actual = {"path": str(path.absolute() if aliases else path.resolve()),
                  "sha256": self.sha(path), "size": stat.st_size,
                  "mtime_ns": stat.st_mtime_ns, "ctime_ns": stat.st_ctime_ns}
        if aliases:
            actual.update(resolved_path=str(path.resolve()),
                          symlink_target=os.readlink(path) if path.is_symlink() else None)
        self.check(actual == frozen, "frozen_file_inventory", path)

    def times(self, row, context):
        values = [row[k] for k in ("wall_seconds", "cpu_user", "cpu_system")]
        self.check(all(type(x) in (int, D) and D(x).is_finite() and x >= 0 for x in values)
                   and values[0] > 0, "valid_times", context)

    def success(self, row, context):
        self.check(row.get("pass") is True and type(row.get("exit")) is int
                   and row["exit"] == 0 and row.get("timeout") is False,
                   "successful_process", context)
        self.times(row, context)

    def logs(self, row, directory, context, native=False):
        for stream in ("stdout", "stderr"):
            relative = Path(row[stream])
            self.check(not relative.is_absolute() and len(relative.parts) == 1,
                       "log_relative_path", context)
            path = directory / relative
            self.check(self.sha(path) == row[stream + "_sha256"], "raw_log_hash", context)
        if native:
            self.check(row["stderr_sha256"] == EMPTY, "native_empty_stderr", context)

    def native(self, row, plan, directory, input_name, expected_name, executable, context):
        self.success(row, context)
        self.logs(row, directory, context, native=True)
        fixtures = plan["fixtures_sha256"]
        self.check(row["input"] == input_name and row["expected"] == expected_name,
                   "oracle_identity", context)
        self.check(row["input_sha256"] == row["input_after_sha256"] == fixtures[input_name],
                   "input_hash_unchanged", context)
        self.check(row["stdout_sha256"] == row["expected_stdout_sha256"] == fixtures[expected_name],
                   "native_matches_frozen_oracle", context)
        self.check(row["stdout_bytes"] == (directory / row["stdout"]).stat().st_size,
                   "native_output_length", context)
        self.check(row["executable"] == row["executable_after"] == executable,
                   "native_executable_unchanged", context)


def expected_argv(plan, arm, work):
    # Explicit command contract, constructed here without the author's command().
    if arm == "A":
        args = [plan["binaries"]["A"]["path"], str(work / "src/main.rs"),
                "--edition=2024", "--crate-name", "zil_cp08_ref", "-Awarnings",
                "--target", "x86_64-unknown-linux-gnu"]
        for option in ("target-cpu=x86-64", "opt-level=" + plan["profile"][1:],
                       "debuginfo=0", "overflow-checks=on", "panic=abort", "embed-bitcode=no"):
            args.extend(("-C", option))
        args.extend(("-o", str(work / "program")))
        if plan["cache_mode"] == "incremental":
            args.extend(("-C", "incremental=" + str(work / "rust-incremental")))
        return args
    args = [plan["binaries"]["B"]["path"], "build", str(work / "src/main.zil"),
            "-o", str(work / "program"), "--json", "--backend", "llvm-api",
            "--clang", plan["binaries"]["clang"]["path"], "--opt", plan["profile"],
            "--codegen-units", str(plan["units"]), "--codegen-jobs", str(plan["jobs"])]
    if plan["cache_mode"] == "incremental":
        args.extend(("--cache-dir", str(work / "source-candidates"),
                     "--object-cache", str(work / "objects")))
    if plan["query_cache"]:
        args.extend(("--query-cache", str(work / "queries")))
    if plan["closed_executable"]:
        args.append("--closed-executable")
    return args


def audit_plan(audit, path, config, repository):
    plan = read(path)
    profile, cache, units, jobs, query, closed, _ = config
    audit.check(plan["schema"] == "zil-rust-compile-performance/2", "plan_schema", path)
    for key, value in {"profile": profile, "cache_mode": cache, "units": units,
                       "jobs": jobs, "query_cache": query, "closed_executable": closed,
                       "blocks": 5, "seed": 20261030, "cpus": [0, 1, 2, 3], "cpu": 0,
                       "target": "x86_64-unknown-linux-gnu", "rust_profile": "native",
                       "rust_linker": None, "selected_original_weight": D(1)}.items():
        audit.check(plan[key] == value and (type(value) is not bool or plan[key] is value),
                    "preregistered_configuration", (path.name, key))
    audit.check({c["id"]: c["weight"] for c in plan["cases"]} == CASE_WEIGHTS,
                "frozen_case_weights", path)
    audit.check(plan["request_weights"] == (REQUEST_WEIGHTS if cache == "incremental"
                                           else {"cold_process": D(1)}), "frozen_request_weights", path)
    audit.check(list(plan["request_weights"]) == (list(REQUEST_WEIGHTS) if cache == "incremental"
                                                else ["cold_process"]), "request_sequence_order", path)
    audit.check(plan["source_identity"] == {"B": {"commit": SOURCE, "crates_tree": CRATES}}
                and plan["binaries"]["B"]["sha256"] == BINARY, "frozen_candidate", path)
    tree = subprocess.check_output(["git", "-C", str(repository), "rev-parse", SOURCE + ":crates"], text=True).strip()
    audit.check(tree == CRATES, "source_commit_crates_tree", SOURCE)
    files = dict(plan["helper_sha256"])
    files["tools/research/compile-performance/rust_bench.py"] = plan["runner_sha256"]
    for name, digest in files.items():
        blob = subprocess.check_output(["git", "-C", str(repository), "show", plan["runner_source_commit"] + ":" + name])
        audit.check(hashlib.sha256(blob).hexdigest() == digest, "frozen_git_helper", name)
        audit.check(audit.sha(repository / name) == digest, "current_helper_matches_frozen", name)
    for tool in [*plan["binaries"].values(), *plan["extra_tools"]]:
        audit.inventory(tool, aliases=True)
    linkage = plan["linkage"]
    candidates = {name: shutil.which(name, path=linkage["linker_search_path"])
                  for name in ("cc", "gcc", "ld", "ld.lld", "as")}
    sibling = Path(plan["binaries"]["clang"]["path"]).parent / "ld.lld"
    candidates.update(clang_sibling_lld=str(sibling) if sibling.is_file() else None,
                      rust_explicit=None, zil_explicit=None)
    audit.check(linkage["zil_linker_override"] is None and linkage["linker_candidates"] == candidates,
                "linker_alias_selection", path)
    expected_names = set(plan["fixtures_sha256"])
    actual_names = {str(p.relative_to(path.parent)) for folder in ("corpus", "inputs", "expected")
                    for p in (path.parent / folder).rglob("*") if p.is_file()}
    audit.check(actual_names == expected_names, "complete_fixture_manifest", path)
    for name, digest in plan["fixtures_sha256"].items():
        audit.check(audit.sha(path.parent / name) == digest, "frozen_fixture_hash", name)
    expected_order = []
    rng = random.Random(20261030)
    for block in range(5):
        cases = list(CASE_WEIGHTS)
        rng.shuffle(cases)
        for case in cases:
            for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"):
                expected_order.append(dict(block=block, case=case, position=position, arm=arm))
    audit.check(plan["order"] == expected_order, "seeded_abba_baab_order", path)
    return plan


def audit_zil_report(audit, row, plan, logs, context):
    report = read(logs / row["stdout"])
    audit.check(report["status"] == "Success" and report["phase"] == "link"
                and not report["diagnostics"], "raw_complete_build_success", context)
    audit.check(type(report["verification_runs"]) is int and report["verification_runs"] == 1,
                "fresh_verify_exactly_once", context)
    audit.check("timings" in report and report["timings"] is None, "phase_timings_disabled", context)
    audit.check(row["reported"] == {k: report[k] for k in row["reported"]},
                "record_matches_raw_report", context)
    work = Path(row["work"])
    audit.check(report["artifacts"] == {"executable": str(work / "program"), "llvm": str(work / "program.ll")},
                "published_artifact_paths", context)
    audit.check(bool(HEX.fullmatch(row["llvm_sha256"])), "llvm_sidecar_digest_present", context)
    native = report["native_codegen"]
    audit.check(native["backend"] == "llvm-api" and native["fallback"] is None
                and native["requested_units"] == plan["units"]
                and type(native["actual_units"]) is int and 0 < native["actual_units"] <= plan["units"]
                and native["jobs"] == min(plan["jobs"], native["actual_units"]),
                "actual_codegen_configuration", context)
    count = native["internalized_functions"]
    audit.check(native["closed_executable"] is plan["closed_executable"]
                and type(count) is int and count >= 0 and (plan["closed_executable"] or count == 0),
                "actual_closed_mode", context)
    audit.check(len(native["object_reports"]) == native["actual_units"], "object_report_coverage", context)
    for obj in native["object_reports"]:
        audit.check(obj["backend"] == "llvm-api" and obj["llvm_version"] == "23.1.4"
                    and obj["triple"] == plan["target"] and obj["object_bytes"] > 0,
                    "actual_object_target", context)
        if obj["input_kind"] == "object-cache":
            # Trusted object reuse has its own explicit report. Current language
            # Verify is independently required above; do not invent LLVM work.
            audit.check(plan["cache_mode"] == "incremental" and obj["cache"]["state"] == "hit"
                        and obj["cache"]["corrupt"] is False and bool(HEX.fullmatch(obj["cache"]["action_key"]))
                        and obj["producer_verified_before_after"] is True
                        and obj["llvm_verification_this_request"] is False
                        and obj["cache"]["current_preflight_ns"] >= 0
                        and all(obj[k] == 0 for k in ("builder_ns", "parse_ns", "optimize_ns", "emit_ns")),
                        "actual_cached_object_contract", context)
        else:
            audit.check(obj["input_kind"] == "builder" and obj["pipeline"] == "default<" + plan["profile"] + ">"
                        and obj["cpu"] == "x86-64" and obj["features"] == ""
                        and obj["verify_before_after"] is True, "actual_fresh_object_pipeline", context)
    cache = native["object_cache"]
    incremental = plan["cache_mode"] == "incremental"
    audit.check(cache["requested"] is incremental and cache["enabled"] is incremental
                and cache["corrupt"] == 0 and cache["write_failures"] == 0
                and cache["disabled_reason"] is None, "object_cache_mode_and_health", context)
    if incremental:
        audit.check(cache["hits"] + cache["misses"] == native["actual_units"]
                    and cache["stored"] == cache["misses"]
                    and sum(obj["input_kind"] == "object-cache" for obj in native["object_reports"]) == cache["hits"],
                    "object_cache_accounting", context)
        if row["request"] == "cold_process":
            audit.check(cache["hits"] == 0, "cold_object_miss", context)
        if row["request"] == "unchanged":
            audit.check(cache["hits"] == native["actual_units"], "unchanged_object_hit", context)
    else:
        audit.check(all(cache[k] == 0 for k in ("hits", "misses", "stored")), "no_cold_compiler_cache", context)
    query = report["query_cache"]
    if plan["query_cache"]:
        hit = row["request"] == "unchanged"
        audit.check(query["requested"] is True and query["state"] == ("hit" if hit else "miss")
                    and query["stored"] is (not hit) and query["reason"] is None
                    and query["write_reason"] is None and bool(HEX.fullmatch(query["key"]))
                    and all(type(query[k]) is int and query[k] >= 0
                            for k in ("ct_steps", "ct_nodes", "ct_functions")),
                    "query_expected_miss_hit_and_usage", context)
        audit.check(report["disk_cache"] in ("missing", "hit"), "source_candidate_cache_healthy", context)
    else:
        audit.check(query["requested"] is False and query["state"] == "disabled"
                    and query["stored"] is False and query["key"] is None
                    and report["disk_cache"] == "disabled", "query_and_source_cache_disabled", context)
    events = report["query_events"]
    audit.check(sum(e == {"phase": "verify", "disposition": "executed"} for e in events) == 1
                and sum(e["phase"] == "verify" for e in events) == 1,
                "verify_event_independent", context)
    reused = plan["query_cache"] and row["request"] == "unchanged"
    for phase in ("expand", "ct", "types", "analyze"):
        audit.check([e["disposition"] for e in events if e["phase"] == phase]
                    == ["reused" if reused else "executed"], "frontend_reuse_events", (context, phase))
    commands = report["commands"]
    audit.check(len(commands) == 1, "full_build_link_command", context)
    for command in commands:
        objects = [x for x in command if x.endswith(".o")]
        audit.check(command[0] == plan["binaries"]["clang"]["path"]
                    and command[1:3] == ["--target=" + plan["target"], "-" + plan["profile"]]
                    and len(objects) == native["actual_units"] and "-o" in command
                    and all(str(work) + "/" in obj for obj in objects)
                    and command[-1] == "--ld-path=" + plan["linkage"]["linker_candidates"]["clang_sibling_lld"],
                    "actual_clang_lld_link", context)
    return report


def audit_build(audit, plan, root, directory, order, sample_kind):
    observed = records(directory / "records.jsonl")
    expected = [(seq, request, sequence) for sequence, seq in enumerate(order)
                for request in plan["request_weights"]]
    audit.check(len(observed) == len(expected), "build_record_count", directory)
    cases = {c["id"]: c for c in plan["cases"]}
    reports, sequences, source_seen = [], defaultdict(dict), defaultdict(set)
    for index, (row, (seq, request, sequence)) in enumerate(zip(observed, expected)):
        context = f"{directory.name}:{index}"
        audit.success(row, context)
        audit.check(all(row[k] == v for k, v in seq.items()) and row["request"] == request
                    and row["index"] == index and row["sample_kind"] == sample_kind
                    and row["plan_sha256"] == audit.sha(root / "plan.json"), "build_record_order_and_binding", context)
        snapshot = SNAPSHOTS[request]
        work = directory / f"work-{sequence:04d}"
        audit.check(row["work"] == str(work) and row["argv"] == expected_argv(plan, row["arm"], work)
                    and row["snapshot"] == snapshot, "exact_build_argv_and_snapshot", context)
        case = cases[row["case"]]
        prefix = case["root"] + "/" + snapshot + ("/rust/" if row["arm"] == "A" else "/zil/")
        sources = {name[len(prefix):]: digest for name, digest in plan["fixtures_sha256"].items() if name.startswith(prefix)}
        audit.check(sources and row["source_before"] == row["source_after"] == sources,
                    "source_bytes_snapshot_unchanged", context)
        audit.logs(row, directory / "logs", context)
        audit.check(row["stdout"] == f"{index:05d}.build.stdout" and row["stderr"] == f"{index:05d}.build.stderr",
                    "build_log_names", context)
        exe = row["executable"]
        audit.check(exe["path"] == str(work / "program") and bool(HEX.fullmatch(exe["sha256"]))
                    and type(exe["size"]) is int and exe["size"] > 0, "built_executable_receipt", context)
        if row["arm"] == "B":
            report = audit_zil_report(audit, row, plan, directory / "logs", context)
            source_files = report["source_files"]
            source_order = [source_files[str(i)] for i in range(len(source_files))]
            relative_order = [str(Path(path).relative_to(work / "src")) for path in source_order]
            audit.check(set(relative_order) == set(sources), "raw_discovered_source_set", context)
            if plan["cache_mode"] == "incremental":
                # Legacy disk_cache is overwritten once per discovered module;
                # its final value describes the last module, not whole-input reuse.
                last = relative_order[-1]
                expected_disk = "hit" if (last, sources[last]) in source_seen[sequence] else "missing"
                audit.check(report["disk_cache"] == expected_disk, "source_candidate_last_module_state", context)
                source_seen[sequence].update(sources.items())
            reports.append(report)
            sequences[sequence][request] = report
        audit.check(len(row["checks"]) == len(case["inputs"]), "complete_short_oracle_set", context)
        for check, sample in zip(row["checks"], case["inputs"]):
            audit.native(check, plan, directory / "logs", sample["path"], sample["expected"][snapshot], exe, context)
    if plan["query_cache"]:
        for sequence, seq in sequences.items():
            cache = {k: v["query_cache"] for k, v in seq.items()}
            audit.check(cache["cold_process"]["key"] == cache["unchanged"]["key"]
                        and len({cache[k]["key"] for k in ("cold_process", "private_edit", "public_edit")}) == 3,
                        "whole_input_query_key_sequence", (directory, sequence))
            audit.check(all(cache["cold_process"][k] == cache["unchanged"][k]
                            for k in ("ct_steps", "ct_nodes", "ct_functions")),
                        "hit_replays_same_ct_usage", (directory, sequence))
    return observed, reports


def audit_receipt(audit, receipt_path, record_path, plan_hash, rows, kind=None, preflight_hash=None):
    receipt = read(receipt_path)
    audit.check(receipt["pass"] is True and receipt["failures"] == [] and receipt["rows"] == len(rows)
                and receipt["plan_sha256"] == plan_hash
                and receipt["records_sha256"] == audit.sha(record_path), "completion_receipt", receipt_path)
    if kind:
        audit.check(receipt["sample_kind"] == kind and receipt["preflight_sha256"] == preflight_hash,
                    "completion_kind_and_preflight", receipt_path)
    return receipt


def median5(values):
    if len(values) != 5:
        raise ValueError("Exactly five paired block means required")
    return sorted(values)[2]


def statistics_from_records(observed, plan, kind):
    request_weights = plan["request_weights"] if kind == "build" else {"runtime": D(1)}
    raw = defaultdict(list)
    for row in observed:
        request = row["request"] if kind == "build" else "runtime"
        for metric, value in (("wall", row["wall_seconds"]),
                              ("cpu", D(row["cpu_user"]) + D(row["cpu_system"]))):
            raw[metric, row["case"], request, row["arm"], row["block"]].append(D(value))
    means = {}
    for metric, case, request, arm, block in raw:
        values = raw[metric, case, request, arm, block]
        if len(values) != 2:
            raise ValueError("Paired block must contain two observations of each arm")
        means[metric, case, request, arm, block] = (values[0] + values[1]) / 2
    components = {"overall": [(c, r, cw * rw) for c, cw in CASE_WEIGHTS.items() for r, rw in request_weights.items()]}
    for case in CASE_WEIGHTS:
        components["case/" + case] = [(case, r, rw) for r, rw in request_weights.items()]
    for request in request_weights:
        components["request/" + request] = [(c, request, cw) for c, cw in CASE_WEIGHTS.items()]
    for case in CASE_WEIGHTS:
        for request in request_weights:
            components["cell/" + case + "/" + request] = [(case, request, D(1))]
    components["without_tree1024x8"] = [(c, r, cw / D(".7") * rw) for c, cw in CASE_WEIGHTS.items()
                                        if c != "tree1024x8" for r, rw in request_weights.items()]
    components["retained_only"] = components["case/retained"]

    cache = {}
    def evaluate(metric, selection):
        key = (metric, tuple(sorted(selection)))
        if key not in cache:
            central = {(case, request, arm): median5([means[metric, case, request, arm, b] for b in selection])
                       for case in CASE_WEIGHTS for request in request_weights for arm in "AB"}
            values = {}
            for label, terms in components.items():
                a, b = (sum((weight * central[case, request, arm] for case, request, weight in terms), D(0)) for arm in "AB")
                values[label] = {"A": a, "B": b, "B_over_A": b / a}
            cache[key] = values
        return cache[key]

    points = {metric: evaluate(metric, range(5)) for metric in ("wall", "cpu")}
    rng = random.Random(20261030)
    selections = {
        "seeded_2000": [rng.choices(range(5), k=5) for _ in range(2000)],
        "exhaustive_3125": list(itertools.product(range(5), repeat=5)),
    }
    intervals = {}
    for method, draws in selections.items():
        sampled = [evaluate("wall", draw) for draw in draws]
        lo, hi = (int((D(p) * len(draws)).to_integral_value(rounding=ROUND_CEILING)) - 1 for p in (".025", ".975"))
        intervals[method] = {}
        for label in components:
            ratios = sorted(sample[label]["B_over_A"] for sample in sampled)
            intervals[method][label] = [ratios[lo], ratios[hi]]
    block_means = {metric: {case: {request: {arm: [means[metric, case, request, arm, block] for block in range(5)]
                                                  for arm in "AB"} for request in request_weights}
                           for case in CASE_WEIGHTS} for metric in ("wall", "cpu")}
    return {"point_estimates": points, "wall_ratio_descriptive_intervals": intervals,
            "paired_block_means_seconds": block_means}


def compare_summary(audit, original, computed, label, kind):
    differences = []
    def same(actual, expected, name):
        delta = abs(D(actual) - expected)
        differences.append(delta)
        audit.check(delta <= max(D(1), abs(expected)) * D("1e-12"), "original_summary_agreement", label + ":" + name)
    for metric in ("wall", "cpu"):
        for key in ("A", "B", "B_over_A"):
            same(original[metric][key], computed["point_estimates"][metric]["overall"][key], metric + "." + key)
        for case in CASE_WEIGHTS:
            for key in ("A", "B", "B_over_A"):
                same(original[metric]["by_case"][case][key], computed["point_estimates"][metric]["case/" + case][key], metric + "." + case + "." + key)
        if kind == "build":
            for request, value in original[metric]["by_request"].items():
                for key in ("A", "B", "B_over_A"):
                    same(value[key], computed["point_estimates"][metric]["request/" + request][key], metric + "." + request + "." + key)
    for i in range(2):
        same(original["exploratory_paired_block_bootstrap95"][i],
             computed["wall_ratio_descriptive_intervals"]["seeded_2000"]["overall"][i], "bootstrap." + str(i))
    return {"compared_numeric_fields": len(differences), "maximum_absolute_difference": max(differences),
            "tolerance": "1e-12 * max(1, abs(independent Decimal value))"}


def run(args):
    audit = Audit()
    root, repository = args.data_root.resolve(), args.repository.resolve()
    formal = root / "measurements-confirmation"
    output = {"schema": "zil-llvm23-round2-independent-rust/1", "reviewer": "/root/benchmark_audit",
              "scope": "Four Rust/ZIL plans: 700 scored builds, 400 scored native runs; 70 build preflights and 80 startup/warmups audited but unscored",
              "data_root": str(root), "formal_directory": str(formal), "source_commit": SOURCE,
              "crates_tree": CRATES, "binary_sha256": BINARY, "script_sha256": audit.sha(Path(__file__)),
              "arithmetic": "Decimal precision 40, JSON decimal literals preserved; two observations averaged per arm/case/request/block, then median of five block means; weight absolute costs before B/A",
              "bootstrap": "Whole paired block indices resampled jointly across cases/requests/arms. Seeded 2000 replicates reproduce the frozen runner convention; exhaustive 3125 ordered draws provide an additional descriptive check. Nearest-rank 2.5% and 97.5%, zero-based indices 49/1949 and 78/3046. Not product confidence.",
              "case_weights": CASE_WEIGHTS, "incremental_request_weights": REQUEST_WEIGHTS,
              "without_largest_weights": {c: w / D('.7') for c, w in CASE_WEIGHTS.items() if c != 'tree1024x8'},
              "results": {}, "evidence": {}, "limitations": [
                  "Shared physical host; no OS page-cache flush or physical-host exclusivity claim.",
                  "Eighty percent synthetic workload weight; retained is separately disclosed.",
                  "Cold builds retain raw logs, output checks and executable/LLVM digests; per-sequence work products are deliberately deleted by the frozen runner and cannot be rehashed now. Retained preflight executables used for all runtime rows are rehashed.",
                  "Compiler binary is tied to source by the recorded pre-build source/tree, build log and frozen binary digest, not by an independent reproducible rebuild.",
                  "Direct e8-to-current runtime quality comparisons and main bridge belong to other independent reviews.",
                  "Audit author previously implemented rust_bench.py/CP12 adapter; this independent arithmetic script imports no author code. Exact runner implementation already had non-author review f0c9e42cb87ce3257f2fc8ae7ef9175f871ff84c.",
              ]}
    try:
        quiet = read(formal / "quiet-window-complete.json")
        audit.check(quiet["pass"] is True and quiet["formal_build_rows"] == 940
                    and quiet["formal_runtime_rows"] == 600 and quiet["commands"] == 22,
                    "global_quiet_window_complete")
        commands = read(formal / "commands.json")
        audit.check(len(commands) == 22 and all(c["exit"] == 0 for c in commands), "all_window_commands_completed")
        for name in ("quiet-window.json", "quiet-window-complete.json", "commands.json"):
            output["evidence"][str(formal / name)] = audit.sha(formal / name)
        old = root / "measurements"
        old_rows = records(old / "rust-o0-cold-build/records.jsonl")
        old_result = read(old / "partial-NOSCORE.json")
        audit.check(len(old_rows) == old_result["rows"] == 57
                    and old_result["verdict"] == "IncompleteOrFailed_NoScore"
                    and not old_result["coverage_complete"] and not old_result["completion_receipt_matches"]
                    and "wall" not in old_result and not (old / "rust-o0-cold-build/complete.json").exists(),
                    "interrupted_window_preserved_noscore")
        audit.check(old_result["records_sha256"] == audit.sha(old / "rust-o0-cold-build/records.jsonl"),
                    "interrupted_window_records_binding")
        output["excluded_interrupted_window"] = {"directory": str(old), "rows": 57,
                                                 "verdict": old_result["verdict"],
                                                 "summary_sha256": audit.sha(old / "partial-NOSCORE.json"),
                                                 "records_sha256": old_result["records_sha256"],
                                                 "failure_receipt_sha256": audit.sha(old / "FAILED-WINDOW.json")}
        start_path = Path("/tmp/zil-round2-build/start.json")
        build_start = read(start_path)
        audit.check(build_start["source"] == SOURCE and build_start["crates_tree"] == CRATES
                    and build_start["command"] == ["cargo", "build", "--release", "--locked", "--offline", "-p", "zil-cli", "-j", "2"],
                    "release_source_build_receipt")
        log_path = start_path.parent / "release.log"
        audit.check("Finished `release` profile [optimized]" in log_path.read_text(), "release_build_success_log")
        output["evidence"][str(start_path)] = audit.sha(start_path)
        output["evidence"][str(log_path)] = audit.sha(log_path)
        if (start_path.parent / "receipt.json").is_file():
            output["evidence"][str(start_path.parent / "receipt.json")] = audit.sha(start_path.parent / "receipt.json")
        counts = Counter()
        for label, config in CONFIGS.items():
            print("audit", label, flush=True)
            plan_path, preflight = root / (label + "-plan/plan.json"), root / (label + "-preflight")
            plan = audit_plan(audit, plan_path, config, repository)
            plan_hash, preflight_hash = audit.sha(plan_path), audit.sha(preflight / "result.json")
            pre_order = [dict(block=0, case=c, position=position, arm=arm)
                         for c in CASE_WEIGHTS for position, arm in enumerate("AB")]
            pre_rows, pre_reports = audit_build(audit, plan, plan_path.parent, preflight, pre_order, "preflight-unscored")
            pre_receipt = audit_receipt(audit, preflight / "result.json", preflight / "records.jsonl", plan_hash, pre_rows)
            audit.check(pre_receipt["warmup"] is True and pre_receipt["artifacts_sha256"] == audit.sha(preflight / "artifacts.json"),
                        "preflight_artifact_receipt", label)
            artifacts = read(preflight / "artifacts.json")
            audit.check([(a["case"], a["arm"]) for a in artifacts] == [(c, a) for c in CASE_WEIGHTS for a in "AB"],
                        "complete_base_executable_set", label)
            by_artifact = {}
            cases = {c["id"]: c for c in plan["cases"]}
            for artifact in artifacts:
                case, exe = cases[artifact["case"]], artifact["executable"]
                audit.inventory(exe)
                origin = next(r for r in pre_rows if r["case"] == artifact["case"] and r["arm"] == artifact["arm"] and r["request"] == "cold_process")
                audit.check(exe["sha256"] == origin["executable"]["sha256"] and exe["size"] == origin["executable"]["size"],
                            "runtime_artifact_from_base_preflight", label)
                audit.native(artifact["startup"], plan, preflight / "logs", "inputs/empty.bin", "expected/empty.bin", exe, label + ":startup")
                audit.native(artifact["warmup"], plan, preflight / "logs", case["input"], case["expected"], exe, label + ":warmup")
                by_artifact[artifact["case"], artifact["arm"]] = exe
            counts["unscored_preflight_build_rows"] += len(pre_rows)
            counts["unscored_startup_warmup_rows"] += 2 * len(artifacts)
            for path in (plan_path, preflight / "result.json", preflight / "records.jsonl", preflight / "artifacts.json"):
                output["evidence"][str(path)] = audit.sha(path)
            for kind in ("build", "runtime"):
                directory = formal / (label + "-" + kind)
                if kind == "build":
                    rows, reports = audit_build(audit, plan, plan_path.parent, directory, plan["order"], "build")
                else:
                    rows = records(directory / "records.jsonl")
                    reports = []
                    audit.check(len(rows) == len(plan["order"]), "runtime_record_count", label)
                    for index, (row, seq) in enumerate(zip(rows, plan["order"])):
                        context = f"{label}:runtime:{index}"
                        audit.check(all(row[k] == v for k, v in seq.items()) and row["index"] == index
                                    and row["sample_kind"] == "runtime" and row["plan_sha256"] == plan_hash
                                    and row["preflight_sha256"] == preflight_hash, "runtime_record_order_and_binding", context)
                        case = cases[row["case"]]
                        audit.native(row, plan, directory / "logs", case["input"], case["expected"],
                                     by_artifact[row["case"], row["arm"]], context)
                receipt = audit_receipt(audit, directory / "complete.json", directory / "records.jsonl", plan_hash,
                                        rows, kind, preflight_hash)
                if kind == "build":
                    audit.check(receipt["warmup"] is False, "formal_build_not_warmup", label)
                counts["formal_" + kind + "_rows"] += len(rows)
                command = [c for c in commands if c["label"] == label + "-" + kind]
                audit.check(len(command) == 1 and "--quiet-window" in command[0]["argv"]
                            and str(directory) in command[0]["argv"] and str(plan_path) in command[0]["argv"]
                            and str(preflight / "result.json") in command[0]["argv"], "command_receipt_binds_window", label + ":" + kind)
                summary_path = formal / (label + "-" + kind + "-summary.json")
                original = read(summary_path)
                audit.check(original["coverage_complete"] is True and original["completion_receipt_matches"] is True
                            and original["failures"] == [] and original["rows"] == len(rows)
                            and original["plan_sha256"] == plan_hash and original["preflight_sha256"] == preflight_hash
                            and original["records_sha256"] == audit.sha(directory / "records.jsonl"),
                            "original_summary_receipt_binding", label + ":" + kind)
                computed = statistics_from_records(rows, plan, kind)
                computed["summary_comparison"] = compare_summary(audit, original, computed, label, kind)
                computed["rows"] = len(rows)
                computed["role"] = config[-1]
                computed["plan_sha256"] = plan_hash
                computed["preflight_sha256"] = preflight_hash
                computed["records_sha256"] = audit.sha(directory / "records.jsonl")
                computed["mode"] = dict(zip(("profile", "cache_mode", "units", "jobs", "query_cache", "closed_executable"), config[:6]))
                if reports:
                    computed["observed_zil_report_counts"] = {
                        "fresh_verify_one": len(reports),
                        "query_states": dict(Counter(r["query_cache"]["state"] for r in reports)),
                        "source_candidate_states": dict(Counter(r["disk_cache"] for r in reports)),
                        "object_cache_hits": sum(r["native_codegen"]["object_cache"]["hits"] for r in reports),
                        "object_cache_misses": sum(r["native_codegen"]["object_cache"]["misses"] for r in reports),
                        "closed_mode": dict(Counter(str(r["native_codegen"]["closed_executable"]) for r in reports)),
                    }
                output["results"][label + "-" + kind] = computed
                for path in (directory / "records.jsonl", directory / "complete.json", summary_path):
                    output["evidence"][str(path)] = audit.sha(path)
        audit.check(counts == {"unscored_preflight_build_rows": 70, "unscored_startup_warmup_rows": 80,
                               "formal_build_rows": 700, "formal_runtime_rows": 400}, "exact_audit_scope", counts)
        output["row_counts"] = dict(counts)
    except Exception as error:
        audit.errors.append({"check": "audit_exception", "detail": repr(error)})
    output["audit_checks"] = dict(sorted(audit.counts.items()))
    output["errors"] = audit.errors
    output["status"] = "Pass" if not audit.errors else "Fail_NoScore"
    output["files_hashed"] = len(audit.hashed)
    output["file_bytes_hashed"] = sum(item[0][2] for item in audit.hashed.values())
    output["verified_files_manifest_sha256"] = hashlib.sha256(json.dumps(
        sorted((path, item[0][2], item[1]) for path, item in audit.hashed.items()), separators=(",", ":")).encode()).hexdigest()
    if audit.errors:
        # No partial numeric result can accidentally be promoted as an audited score.
        output.pop("results", None)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("x") as stream:
        json.dump(output, stream, indent=2, default=decimal_json)
        stream.write("\n")
    print(output["status"], "checks", sum(audit.counts.values()), "errors", len(audit.errors), flush=True)
    if audit.errors:
        print(json.dumps(audit.errors[:20], indent=2))
    return 0 if not audit.errors else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-root", type=Path, default=Path("/tmp/zil-llvm23-round2"))
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    sys.exit(run(parser.parse_args()))

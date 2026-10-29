#!/usr/bin/env python3
"""Independent CP04 receipt/byte/arithmetic audit. Executes no measured program.

Uses only Python standard library, never imports the benchmark aggregate.
All statistics use Decimal values parsed directly from recorded JSON numbers.
"""

from collections import Counter, defaultdict
from decimal import Decimal, ROUND_CEILING, getcontext
import hashlib
from itertools import product
import json
from pathlib import Path
import random
import struct
import subprocess
import sys
import traceback


getcontext().prec = 50
BASE = Path(__file__).resolve().parents[1]
REVIEW = BASE / "cp04-review"
PLAN = BASE / "runs/cp04-runtime-plan"
BUILD = BASE / "runs/cp04-runtime-build"
MEASURE = BASE / "runs/cp04-runtime-measure"
REPO = BASE / "zil"
OUTPUT = REVIEW / "independent-runtime-arithmetic.json"
failures = []
check_count = 0
inventory = {}


def check(label, condition):
    global check_count
    check_count += 1
    if not condition:
        failures.append(label)


def load(path):
    return json.loads(path.read_text(), parse_float=Decimal)


def hashed(path):
    path = path.resolve()
    if str(path) not in inventory:
        digest = hashlib.sha256()
        with path.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(block)
        inventory[str(path)] = {"sha256": digest.hexdigest(), "bytes": path.stat().st_size}
    return inventory[str(path)]["sha256"]


def identity(record, label):
    path = Path(record["path"])
    check(label + ":sha256", hashed(path) == record["sha256"])
    stat = path.stat()
    check(label + ":metadata", stat.st_size == record["size"] and
          stat.st_mtime_ns == record["mtime_ns"] and stat.st_ctime_ns == record["ctime_ns"])


def native_row(row, logs, expected_executable, case, label):
    check(label + ":success", row["pass"] is True and row["exit"] == 0 and row["timeout"] is False)
    check(label + ":executable", row["executable"] == row["executable_after"] == expected_executable)
    check(label + ":input-path", row["input"] == case["input"])
    check(label + ":expected-path", row["expected"] == case["expected"])
    check(label + ":input-hash", row["input_sha256"] == row["input_after_sha256"] == hashed(PLAN / case["input"]))
    expected_hash = hashed(PLAN / case["expected"])
    stdout, stderr = logs / row["stdout"], logs / row["stderr"]
    check(label + ":stdout-hash", hashed(stdout) == row["stdout_sha256"] == row["expected_stdout_sha256"] == expected_hash)
    check(label + ":stdout-bytes", stdout.read_bytes() == (PLAN / case["expected"]).read_bytes())
    check(label + ":stdout-size", stdout.stat().st_size == row["stdout_bytes"])
    check(label + ":stderr", hashed(stderr) == row["stderr_sha256"] and stderr.stat().st_size == 0)
    for key in ("wall_seconds", "cpu_user", "cpu_system"):
        value = Decimal(row[key])
        check(label + ":finite-" + key, value.is_finite() and value >= 0)
    check(label + ":positive-time", row["wall_seconds"] > 0 and row["cpu_user"] + row["cpu_system"] > 0)


def elf_sections(path):
    """Read only ELF64 section and function sizes; no execution/disassembly."""
    data = path.read_bytes()
    if data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("expected little-endian ELF64")
    offset = struct.unpack_from("<Q", data, 40)[0]
    width, count, names_index = struct.unpack_from("<HHH", data, 58)
    raw = [struct.unpack_from("<IIQQQQIIQQ", data, offset + i * width) for i in range(count)]
    names = data[raw[names_index][4]:raw[names_index][4] + raw[names_index][5]]
    name = lambda table, pos: table[pos:table.index(0, pos)].decode("utf-8")
    sections = {name(names, row[0]): {"size": row[5], "address": row[3], "alignment": row[8]} for row in raw}
    functions = {}
    for section in raw:
        if section[1] != 2:  # SHT_SYMTAB
            continue
        strings = raw[section[6]]
        table = data[strings[4]:strings[4] + strings[5]]
        for pos in range(section[4], section[4] + section[5], section[9]):
            symbol, info, other, shndx, value, size = struct.unpack_from("<IBBHQQ", data, pos)
            if info & 15 == 2:
                text = name(table, symbol)
                if text.startswith("zil.fn."):
                    functions[text] = size
    return sections, functions


def main():
    report = {
        "schema_version": 1,
        "task": "CP04",
        "reviewer": "/root/backend_audit",
        "runtime_tool_author": "/root/compiler_research",
        "independent_of_runtime_tool_author": True,
        "classification": "Independent byte/receipt and Decimal arithmetic audit of existing observations; no new performance or program execution",
        "script_path": str(Path(__file__)),
        "script_sha256": hashed(Path(__file__)),
        "precision_decimal_digits": 50,
        "status": "started",
        "limitations": [
            "Wall/CPU observations are accepted as recorded; this audit does not independently remeasure them.",
            "Raw stdout is compared byte-for-byte to the frozen expected files; the oracle implementation is not rerun.",
            "Fixed affinity follows the hashed runner code and plan; no new scheduling/performance claims are made.",
            "ELF sizes are static evidence and do not establish cache misses, branch behavior, spill cost, or any causal runtime explanation.",
            "The finite paired bootstrap describes this frozen shared-host sample, not a product-level performance guarantee.",
            "No unrelated 768-run corpus is revalidated. Product acceptance remains NotRun."
        ],
    }
    try:
        plan = load(PLAN / "plan.json")
        build_receipt = load(BUILD / "result.json")
        complete = load(MEASURE / "complete.json")
        summary_path = BASE / "runs/cp04-runtime-summary.json"
        summary = load(summary_path)
        builds = [json.loads(line, parse_float=Decimal) for line in (BUILD / "builds.jsonl").read_text().splitlines()]
        rows = [json.loads(line, parse_float=Decimal) for line in (MEASURE / "records.jsonl").read_text().splitlines()]
        plan_hash, build_hash = hashed(PLAN / "plan.json"), hashed(BUILD / "result.json")
        records_hash, builds_hash = hashed(MEASURE / "records.jsonl"), hashed(BUILD / "builds.jsonl")
        report["inputs"] = {str(path): hashed(path) for path in [PLAN / "plan.json", BUILD / "result.json", BUILD / "builds.jsonl", MEASURE / "complete.json", MEASURE / "records.jsonl", summary_path]}
        check("fixed-protocol", plan["blocks"] == 5 and plan["expected_rows"] == 100 and len(rows) == len(plan["order"]) == 100)
        check("fixed-build-settings", plan["cpu"] == 0 and plan["profile"] == "O0" and plan["units"] == plan["jobs"] == 1 and plan["object_cache"] is False and plan["timings"] is False)
        check("weight-sum", sum(c["weight"] for c in plan["cases"]) == 1)
        check("build-receipt", build_receipt == {"plan_sha256": plan_hash, "rows": 10, "pass": True, "failures": [], "builds_sha256": builds_hash, "runtime_samples": 0})
        check("completion-receipt", complete == {"plan_sha256": plan_hash, "build_receipt_sha256": build_hash, "rows": 100, "failures": [], "records_sha256": records_hash})
        check("summary-bindings", summary["plan_sha256"] == plan_hash and summary["build_receipt_sha256"] == build_hash and summary["records_sha256"] == records_hash and summary["source_identity"] == plan["source_identity"])
        expected_fixture_names = set(plan["fixtures_sha256"])
        actual_fixture_names = {str(p.relative_to(PLAN)) for folder in ("sources", "inputs", "expected") for p in (PLAN / folder).rglob("*") if p.is_file()}
        check("exact-fixture-set", actual_fixture_names == expected_fixture_names)
        for name, expected in plan["fixtures_sha256"].items():
            check("fixture:" + name, hashed(PLAN / name) == expected)
        for key, binary in plan["binaries"].items():
            identity(binary, "compiler:" + key)
        for binary in plan["extra_tools"]:
            identity(binary, "tool:" + binary["path"])
        check("runner-hash", hashed(REPO / "tools/research/compile-performance/runtime_bench.py") == plan["runner_sha256"])
        for name, expected in plan["helper_sha256"].items():
            check("helper:" + name, hashed(REPO / name) == expected)
        check("upstream-plan", hashed(Path(plan["upstream_corpus_plan"]["path"])) == plan["upstream_corpus_plan"]["sha256"])
        source_map_path = REVIEW / "source-map.json"
        source_map = load(source_map_path)
        report["source_map_sha256"] = hashed(source_map_path)
        for arm, mapping in zip("AB", source_map):
            frozen = plan["source_identity"][arm]
            canonical = subprocess.check_output(["git", "rev-parse", mapping["canonical_source_commit"] + ":crates"], cwd=REPO, text=True).strip()
            author = subprocess.check_output(["git", "rev-parse", mapping["build_source_commit"] + ":crates"], cwd=REPO, text=True).strip()
            check("source-map:" + arm, canonical == author == mapping["crates_tree"] == frozen["crates_tree"] and mapping["canonical_source_commit"] == frozen["commit"] and mapping["sha256"] == plan["binaries"][arm]["sha256"])
        cases = {c["id"]: c for c in plan["cases"]}
        check("ten-ordered-builds", [(r["case"], r["arm"]) for r in builds] == [(c["id"], arm) for c in plan["cases"] for arm in "AB"])
        artifacts = {(r["case"], r["arm"]): r for r in builds}
        for build in builds:
            cid, arm = build["case"], build["arm"]
            case = cases[cid]
            label = "build:" + cid + ":" + arm
            identity(build["executable"], label)
            check(label + ":source", build["source_before"] == build["source_after"] == case["source_sha256"])
            check(label + ":flags", build["pass"] is True and build["plan_sha256"] == plan_hash and build["compiler_status"] == "Success" and build["verification_runs"] == 1 and build["timings_disabled"] is True)
            check(label + ":process", build["build_process"]["exit"] == 0 and build["build_process"]["timeout"] is False and build["build_process"]["argv"] == build["argv"])
            expected_argv = [plan["binaries"][arm]["path"], "build", str(PLAN / case["source"] / "main.zil"), "-o", build["executable"]["path"], "--json", "--backend", "llvm-api", "--opt", "O0", "--codegen-units", "1", "--codegen-jobs", "1", "--clang", plan["binaries"]["clang"]["path"]]
            check(label + ":argv", build["argv"] == expected_argv)
            for stream in ("stdout", "stderr"):
                check(label + ":raw-" + stream, hashed(BUILD / "logs" / build[stream]) == build[stream + "_sha256"])
            raw = load(BUILD / "logs" / build["stdout"])
            native = raw["native_codegen"]
            check(label + ":production-report", raw["status"] == "Success" and raw["verification_runs"] == 1 and raw["timings"] is None and native["backend"] == "llvm-api" and native["requested_units"] == native["actual_units"] == native["jobs"] == 1 and native["fallback"] is None and native["object_cache"]["enabled"] is False and native["object_cache"]["requested"] is False)
            check(label + ":recipe", len(native["object_reports"]) == 1 and native["object_reports"][0]["pipeline"] == ("default<O0>" if arm == "A" else "default<O0>,function(mem2reg)") and native["object_reports"][0]["input_kind"] == "builder" and native["object_reports"][0]["verify_before_after"] is True and native["object_reports"][0]["parse_ns"] == 0)
            native_row(build["startup"], BUILD / "logs", build["executable"], {"input": "inputs/empty.bin", "expected": "expected/empty.bin"}, label + ":startup")
            native_row(build["warmup"], BUILD / "logs", build["executable"], case, label + ":warmup")
        groups = defaultdict(list)
        seen = set()
        for index, row in enumerate(rows):
            seq = {key: row[key] for key in ("block", "case", "position", "arm")}
            check("row-order:" + str(index), seq == plan["order"][index] and row["index"] == index)
            key = tuple(seq[k] for k in ("block", "case", "position", "arm"))
            check("row-unique:" + str(index), key not in seen)
            seen.add(key)
            check("row-binding:" + str(index), row["plan_sha256"] == plan_hash and row["build_receipt_sha256"] == build_hash)
            native_row(row, MEASURE / "logs", artifacts[row["case"], row["arm"]]["executable"], cases[row["case"]], "row:" + str(index))
            groups[row["case"], row["block"]].append(row)
        block_means = {}
        for cid in cases:
            for block in range(5):
                entries = sorted(groups[cid, block], key=lambda r: r["position"])
                check("ABBA-BAAB:" + cid + ":" + str(block), [r["position"] for r in entries] == [0, 1, 2, 3] and "".join(r["arm"] for r in entries) == ("ABBA" if block % 2 == 0 else "BAAB"))
                for arm in "AB":
                    pair = [r for r in entries if r["arm"] == arm]
                    check("two-replicates:" + cid + ":" + str(block) + ":" + arm, len(pair) == 2)
                    block_means[cid, block, arm] = {
                        "wall": sum(r["wall_seconds"] for r in pair) / 2,
                        "cpu": sum(r["cpu_user"] + r["cpu_system"] for r in pair) / 2,
                    }

        def calculate(selection, metric):
            totals = {arm: Decimal(0) for arm in "AB"}
            per_case = {}
            for case in plan["cases"]:
                # Fixed protocol: median of five two-replicate means for each arm.
                values = {arm: sorted(block_means[case["id"], b, arm][metric] for b in selection)[2] for arm in "AB"}
                for arm in "AB":
                    totals[arm] += case["weight"] * values[arm]
                per_case[case["id"]] = {**values, "B_over_A": values["B"] / values["A"]}
            return {**totals, "B_over_A": totals["B"] / totals["A"], "by_case": per_case}

        statistics = {metric: calculate(range(5), metric) for metric in ("wall", "cpu")}
        tolerance = Decimal("1e-14")
        for metric, actual in statistics.items():
            for key in ("A", "B", "B_over_A"):
                check("summary:" + metric + ":" + key, abs(actual[key] - summary[metric][key]) <= tolerance)
            for cid in cases:
                for key in ("A", "B", "B_over_A"):
                    check("summary:" + metric + ":" + cid + ":" + key, abs(actual["by_case"][cid][key] - summary[metric]["by_case"][cid][key]) <= tolerance)
        distribution = sorted(calculate(draw, "wall")["B_over_A"] for draw in product(range(5), repeat=5))
        quantiles = [distribution[int((q * len(distribution)).to_integral_value(rounding=ROUND_CEILING)) - 1] for q in (Decimal(".025"), Decimal(".975"))]
        rng = random.Random(plan["seed"])
        monte_carlo = sorted(calculate(rng.choices(range(5), k=5), "wall")["B_over_A"] for _ in range(5000))
        mc_quantiles = [monte_carlo[124], monte_carlo[4874]]
        for i, value in enumerate(mc_quantiles):
            check("MC-endpoint:" + str(i), abs(value - summary["exploratory_paired_block_bootstrap95"][i]) <= tolerance)
        report["arithmetic"] = {
            "method": "Within each case/block/arm take mean of two observations; for each case/arm take median of five block means; sum frozen weighted case medians separately for A and B; report B total/A total. No startup subtraction and no trimming.",
            **statistics,
            "frozen_case_weights": {c["id"]: c["weight"] for c in plan["cases"]},
            "block_means": [{"case": cid, "block": block, "arm": arm, **values} for (cid, block, arm), values in sorted(block_means.items())],
            "paired_bootstrap": {"resampling_unit": "One common vector of five block IDs, reused for every case and both arms", "exact_ordered_draws": len(distribution), "exact_nearest_rank95": quantiles, "monte_carlo_draws": 5000, "monte_carlo_seed": plan["seed"], "independent_MC95": mc_quantiles, "reported_MC95": summary["exploratory_paired_block_bootstrap95"], "total_decimal_comparison_tolerance": tolerance},
        }
        static = {}
        for cid in cases:
            a_path, b_path = (Path(artifacts[cid, arm]["executable"]["path"]) for arm in "AB")
            a_sections, a_functions = elf_sections(a_path)
            b_sections, b_functions = elf_sections(b_path)
            common = a_functions.keys() & b_functions.keys()
            hist = Counter(b_functions[name] - a_functions[name] for name in common)
            static[cid] = {
                "executable_bytes": {"A": a_path.stat().st_size, "B": b_path.stat().st_size, "B_minus_A": b_path.stat().st_size - a_path.stat().st_size},
                "sections": {name: {"A": a_sections.get(name, {}).get("size", 0), "B": b_sections.get(name, {}).get("size", 0), "B_minus_A": b_sections.get(name, {}).get("size", 0) - a_sections.get(name, {}).get("size", 0)} for name in sorted(a_sections.keys() | b_sections.keys()) if name},
                "zil_function_symbols": {"A": len(a_functions), "B": len(b_functions), "common": len(common), "size_delta_histogram": dict(sorted(hist.items()))},
            }
        report["static_elf_observations"] = static
        report["raw_evidence_coverage"] = {"planned_fixture_files": len(expected_fixture_names), "builds": len(builds), "startup_diagnostics": 10, "warmup_runs": 10, "measured_rows": len(rows), "unique_rows": len(seen), "case_blocks": len(groups), "two_replicate_cells": len(block_means), "all_failures_retained": failures}
        impact_path = BASE / "cp04-logs/04-root-impact-701dd6c18.json"
        impact = load(impact_path)
        impact_log = Path(impact["log"])
        check("root-impact-receipt", impact["exit"] == 0 and impact["source_commit"] == impact["source_commit_after"] == "701dd6c18020607a4d89d51239c9dba59ea90e33" and hashed(impact_log) == impact["log_sha256"])
        text = impact_log.read_text()
        check("root-impact-count", text.count("test result: ok. 11 passed; 0 failed;") == 2 and text.count("test result: ok. 5 passed; 0 failed;") == 1)
        report["root_execution_evidence"] = {"executor": "/root", "receipt_path": str(impact_path), "receipt_sha256": hashed(impact_path), "log_sha256": hashed(impact_log), "passed": 27, "failed": 0, "reviewer_reexecuted": False}
        report["verdict"] = "Runtime arithmetic and file binding verified; measured runtime regression supports candidate NoGo. Compilation benefit adjudication is separate and was not recomputed here."
        report["status"] = "Pass" if not failures else "Fail"
    except Exception as error:
        report.update(status="Fail", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        report["check_count"] = check_count
        report["failures"] = failures
        report["verified_file_count"] = len(inventory)
        serialized_inventory = json.dumps(inventory, sort_keys=True, separators=(",", ":"))
        (REVIEW / "runtime-verified-files.json").write_text(json.dumps(inventory, indent=2) + "\n")
        report["verified_file_inventory"] = {"path": str(REVIEW / "runtime-verified-files.json"), "canonical_content_sha256": hashlib.sha256(serialized_inventory.encode()).hexdigest()}
        OUTPUT.write_text(json.dumps(report, indent=2, default=str) + "\n")
    print(json.dumps({"path": str(OUTPUT), "status": report["status"], "failures": failures, "check_count": check_count, "verified_files": len(inventory), "wall_ratio": str(report["arithmetic"]["wall"]["B_over_A"]), "cpu_ratio": str(report["arithmetic"]["cpu"]["B_over_A"]), "exact_CI": [str(x) for x in quantiles]}, indent=2))
    return 0 if report["status"] == "Pass" else 1


if __name__ == "__main__":
    sys.exit(main())

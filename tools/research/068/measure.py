#!/usr/bin/env python3
"""Frozen, paired, complete compile_path observations; no instrumented timings.

Build two binaries before running this controller. Both must link this exact
runner to the stated production source revision. An exclusive sampling window
must be arranged externally. A warm-up is retained but excluded, never erased.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def invoke(binary, directory, case, mode, source):
    command = [str(binary), str(directory), mode, str(source / case["root"])]
    before = time.monotonic_ns()
    result = subprocess.run(command, capture_output=True, timeout=45)
    elapsed = time.monotonic_ns() - before
    # Even an early failure has a full retained command/output record.
    directory.mkdir(exist_ok=True)
    (directory / "runner.stdout").write_bytes(result.stdout)
    (directory / "runner.stderr").write_bytes(result.stderr)
    invocation = {"argv": command, "cwd": str(Path.cwd()), "exit": result.returncode,
                  "process_wall_ns": elapsed, "binary_sha256": sha(binary)}
    write(directory / "command.json", invocation)
    if result.returncode:
        raise RuntimeError(f"failed compiler runner: {directory}")
    data = json.loads((directory / "report.json").read_text())
    controls = []
    for record in data["records"]:
        report = record["report"]
        assert report["status"] == "Success", (directory, report)
        assert report["verification_runs"] == 1, (directory, report)
        assert record["active_queries"] == 0, directory
        source_hashes = {name: sha(name) for name in report["source_files"].values()}
        artifact_hashes = {name: sha(path) for name, path in report["artifacts"].items()
                           if name != "executable"}
        native = None
        if "executable" in report["artifacts"]:
            executable = report["artifacts"]["executable"]
            run = subprocess.run([executable], input=case["stdin"].encode(),
                                 capture_output=True, timeout=5)
            prefix = directory / record["scenario"]
            prefix.with_suffix(".native.stdout").write_bytes(run.stdout)
            prefix.with_suffix(".native.stderr").write_bytes(run.stderr)
            native = {"exit": run.returncode, "stdout": run.stdout.decode(),
                      "stderr": run.stderr.decode(), "stdin": case["stdin"]}
            assert native == {"exit": case["exit"], "stdout": case["stdout"],
                              "stderr": "", "stdin": case["stdin"]}, (directory, native)
        controls.append({"scenario": record["scenario"], "input": report["input_fingerprint"],
                         "sources": source_hashes, "artifacts": artifact_hashes,
                         "interfaces": report["scalar_interfaces"], "native": native,
                         "query_events": report["query_events"]})
    write(directory / "controls.json", controls)
    return data, invocation, controls


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--baseline-binary", type=Path, required=True)
    parser.add_argument("--candidate-binary", type=Path, required=True)
    parser.add_argument("--baseline-commit", required=True)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=7)
    parser.add_argument("--warmup", type=int, default=1)
    args = parser.parse_args()
    assert args.rounds > 0 and args.warmup >= 0
    # Metadata must resolve to a complete commit in the shared object store.
    # A copied binary's real source identity is also retained in frozen-build.
    for commit in (args.baseline_commit, args.candidate_commit):
        resolved = subprocess.check_output(
            ["git", "-C", str(args.source), "rev-parse", "--verify", f"{commit}^{{commit}}"],
            text=True,
        ).strip()
        assert resolved == commit, "complete, existing commit identity required"
    args.output.mkdir(parents=True, exist_ok=False)
    cases_path = args.source / "tests/perf/impl068/cases.json"
    cases = json.loads(cases_path.read_text())
    binaries = {"baseline": args.baseline_binary.resolve(),
                "candidate": args.candidate_binary.resolve()}
    manifest = {"baseline_commit": args.baseline_commit,
                "candidate_commit": args.candidate_commit,
                "binary_sha256": {name: sha(path) for name, path in binaries.items()},
                "controller_sha256": sha(__file__), "runner_sha256": sha(Path(__file__).with_name("main.rs")),
                "case_manifest_sha256": sha(cases_path), "cases": cases,
                "rounds": args.rounds, "warmup_retained_excluded": args.warmup,
                "modes": ["verify", "build", "build-emit"],
                "order": "paired variants, alternating first by (round + case + mode) parity",
                "host": platform.platform(), "cpu_count": os.cpu_count(),
                "environment": {key: os.environ.get(key) for key in
                                ["CARGO_INCREMENTAL", "CARGO_BUILD_JOBS", "CARGO_PROFILE_DEV_DEBUG",
                                 "CARGO_PROFILE_TEST_DEBUG", "ZIL_CLANG"]},
                "clock": "monotonic nanoseconds; uninstrumented optimized driver",
                "scope": "API full compile_path cold empty session then unchanged session; OS file cache not cleared; process wall additionally includes runner I/O, report serialization and destruction; native execution checks outside all measured intervals"}
    write(args.output / "manifest.json", manifest)
    samples = []
    whole_start = time.monotonic_ns()
    for iteration in range(args.warmup + args.rounds):
        for ci, case in enumerate(cases):
            for mi, mode in enumerate(manifest["modes"]):
                order = ["baseline", "candidate"]
                if (iteration + ci + mi) % 2:
                    order.reverse()
                pair = {}
                for variant in order:
                    directory = args.output / f"r{iteration:02}-{case['name']}-{mode}-{variant}"
                    data, invocation, controls = invoke(binaries[variant], directory, case, mode, args.source)
                    pair[variant] = controls
                    samples.append({"iteration": iteration, "warmup": iteration < args.warmup,
                                    "case": case["name"], "mode": mode, "variant": variant,
                                    "process_wall_ns": invocation["process_wall_ns"], "records": data["records"]})
                    write(args.output / "samples.json", samples)
                assert pair["baseline"] == pair["candidate"], f"semantic/output mismatch: {case['name']} {mode} r{iteration}"
    cells = []
    for case in cases:
        for mode in manifest["modes"]:
            for scenario in ["cold", "unchanged"]:
                by_variant = {}
                for variant in binaries:
                    runs = [s for s in samples if not s["warmup"] and s["case"] == case["name"]
                            and s["mode"] == mode and s["variant"] == variant]
                    observations = [next(r for r in s["records"] if r["scenario"] == scenario) for s in runs]
                    ns = [r["api_wall_ns"] for r in observations]
                    internal = [r["report"]["seconds"] * 1e9 for r in observations]
                    by_variant[variant] = {"api_ns": ns, "median_ns": statistics.median(ns),
                                           "min_ns": min(ns), "max_ns": max(ns),
                                           "report_ns": internal, "median_report_ns": statistics.median(internal),
                                           "process_wall_ns": [s["process_wall_ns"] for s in runs]}
                ratios = [c / b for b, c in zip(by_variant["baseline"]["api_ns"], by_variant["candidate"]["api_ns"])]
                cells.append({"case": case["name"], "mode": mode, "scenario": scenario,
                              "variants": by_variant, "paired_candidate_over_baseline": ratios,
                              "median_paired_ratio": statistics.median(ratios)})
    summary = {"success": True, "comparisons": len(samples) // 2,
               "compiler_requests": 2 * len(samples), "actual_verification_runs": 2 * len(samples),
               "sampling_elapsed_ns": time.monotonic_ns() - whole_start, "cells": cells,
               "limitations": ["single-host bounded study; no universal speedup claim", "cumulative three-change comparison; no attribution by wall-time subtraction", "warm OS files; empty compiler cache only defines cold", "synthetic case is scaling control, not application evidence"]}
    write(args.output / "summary.json", summary)
    print(json.dumps({key: value for key, value in summary.items() if key != "cells"}, indent=2))


if __name__ == "__main__":
    main()

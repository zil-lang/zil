#!/usr/bin/env python3
"""Freeze the original multi-module runtime controls with function merging off.

This is a new, finite quality lane. It does not replace the original after
matrix, alter its thresholds, claim compilation speedups, or grant acceptance.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import time


SPECS = [("dispatch8", 8), ("pipeline8", 8), ("dispatch16", 16), ("pipeline16", 16)]
ARMS = ("base", "cand", "rust")
SNAPSHOTS = ("base", "private")
DEPTHS = (0, 11)


def append(path, value):
    with path.open("a") as stream:
        stream.write(json.dumps(value, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--base-commit", required=True)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--protocol", type=Path,
                        default=Path(__file__).resolve().parent / "helper-current")
    args = parser.parse_args()
    protocol = args.protocol.resolve()
    spec = importlib.util.spec_from_file_location("original_multi", protocol / "multimodule.py")
    multi = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(multi)
    measure = multi.m
    assert multi.SPECS == SPECS, "the original four-case protocol must be unchanged"

    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    assert len(cpus) == 2, "the original protocol requires two logical CPUs"
    os.sched_setaffinity(0, cpus)
    tools = {"base": args.base.resolve(), "cand": args.candidate.resolve(),
             "rust": Path(subprocess.check_output(["rustup", "which", "rustc"], text=True).strip())}
    clang = Path(os.environ["ZIL_CLANG"]).resolve()
    inputs = [bytes([i, 3, 0, 1]) for i in range(16)] + [bytes([73, 11, 0, 3]), bytes([0, 0, 0, 2])]
    manifest = {
        "baseline": args.base_commit, "candidate": args.candidate_commit,
        "scope": "off quality only; four original multi-module cases, no weighted compile claim",
        "specs": SPECS, "snapshots": SNAPSHOTS, "depths": DEPTHS,
        "function_merging": "off", "optimization": "O2", "incremental_o2": True,
        "codegen_units": 8, "codegen_jobs": 2, "cpus": cpus,
        "build_history": "base then private in the same per-case/arm semantic, object and Rust incremental directories",
        "inputs": [value.hex() for value in inputs],
        "calibration": {"arm": "base", "depth": 3, "initial_iterations": 256,
                        "multiplier": 2, "stop_seconds_strictly_above": .15,
                        "maximum_iterations": 16384},
        "runtime": {"warmup_repetitions": 1, "formal_repetitions": 7,
                    "relative_latency_limit": 1.1, "rust_throughput_minimum": .9},
        "tools": {arm: {"path": str(path), "sha256": measure.sha(path)}
                  for arm, path in tools.items()},
        "clang": {"path": str(clang), "sha256": measure.sha(clang)},
        "rust_version": subprocess.check_output([str(tools["rust"]), "-vV"], text=True),
        "protocol_hashes": {name: measure.sha(protocol / name)
                            for name in ["multimodule.py", "bench/measure.py", "bench/corpus.py"]},
        "script_sha256": measure.sha(Path(__file__)),
        "rust_contract": "original fallible reserve and checked doubling; allocator equivalence not claimed",
        "weighted_in_original_protocol": False,
    }
    for kind, n in SPECS:
        for snapshot in SNAPSHOTS:
            for language in ("zil", "rust"):
                folder = out / "corpus" / kind / snapshot / language
                folder.mkdir(parents=True)
                for name, source in multi.files(kind, n, language, snapshot).items():
                    (folder / name).write_text(source)
    manifest["source_hashes"] = {
        str(path.relative_to(out)): measure.sha(path)
        for path in sorted((out / "corpus").rglob("*")) if path.is_file()
    }
    measure.dump(out / "manifest.json", manifest)
    for name in ("rows.jsonl", "runtime.jsonl", "calibration.jsonl"):
        (out / name).touch(exist_ok=False)
    rows, runs, calibration, failures = [], [], [], []
    try:
        for kind, n in SPECS:
            for snapshot in SNAPSHOTS:
                order = list(ARMS)
                random.Random(f"{kind}/{snapshot}/off-build").shuffle(order)
                for arm in order:
                    language = "rust" if arm == "rust" else "zil"
                    extension = "rs" if arm == "rust" else "zil"
                    work = out / "work" / kind / arm
                    work.mkdir(parents=True, exist_ok=True)
                    for name, source in multi.files(kind, n, language, snapshot).items():
                        expected_hash = manifest["source_hashes"][f"corpus/{kind}/{snapshot}/{language}/{name}"]
                        assert hashlib.sha256(source.encode()).hexdigest() == expected_hash
                        (work / name).write_text(source)
                    exe = work / "app"
                    exe.unlink(missing_ok=True)
                    if arm == "rust":
                        argv = [tools[arm], work / "main.rs", "--edition=2024", "--crate-name",
                                "multimodule_handlers", "-A", "warnings", "-C", "opt-level=2",
                                "-C", "debuginfo=0", "-C", "overflow-checks=yes", "-C", "panic=abort",
                                "-C", "embed-bitcode=no", "-C", "incremental=" + str(work / "incremental"),
                                "-o", exe]
                    else:
                        argv = [tools[arm], "build", work / "main.zil", "--json", "--opt", "O2",
                                "--backend", "llvm-api", "--clang", clang, "--codegen-units", "8",
                                "--codegen-jobs", "2", "--semantic-cache", work / "semantic",
                                "--object-cache", work / "objects", "-o", exe,
                                "--incremental-o2", "--function-merging", "off"]
                    row, stdout, stderr = measure.timed(argv, out / "logs" / str(len(rows)), timeout=180)
                    row.update(case=kind, n=n, snapshot=snapshot, arm=arm, mode="O2",
                               function_merging="off" if arm != "rust" else None,
                               valid=False, native_checks=[])
                    rows.append(row)
                    try:
                        assert row["exit"] == 0 and not row["timeout"], row
                        if arm != "rust":
                            report = json.loads(stdout)
                            assert report["status"] == "Success" and report["verification_runs"] == 1, report
                            row["verification_runs"] = report["verification_runs"]
                            row["native_codegen"] = report["native_codegen"]
                            row["actual_units"] = report["native_codegen"]["actual_units"]
                        for data in inputs:
                            result = subprocess.run([str(exe)], input=data, capture_output=True, timeout=30)
                            want = multi.expected(kind, n, snapshot, data)
                            passed = result.returncode == 0 and result.stdout == want and not result.stderr
                            row["native_checks"].append({"input": data.hex(), "expected": want.hex(),
                                                         "actual": result.stdout.hex(), "exit": result.returncode,
                                                         "stderr": result.stderr.hex(), "pass": passed})
                            assert passed, row["native_checks"][-1]
                        row["text_sha256"] = measure.text_hash(exe)
                        row["elf_sha256"] = measure.sha(exe)
                        row["exe_bytes"] = exe.stat().st_size
                        preserved = out / "quality" / kind / snapshot / arm
                        preserved.mkdir(parents=True)
                        shutil.copy2(exe, preserved / "app")
                        assert measure.sha(preserved / "app") == row["elf_sha256"]
                        row["valid"] = True
                    finally:
                        measure.dump(Path(row["log"]) / "record.json", row)
                        append(out / "rows.jsonl", row)
                print(json.dumps({"compiled": kind, "snapshot": snapshot, "requests": len(rows)}), flush=True)

        for kind, n in SPECS:
            for snapshot in SNAPSHOTS:
                root = out / "quality" / kind / snapshot
                iterations = 256
                while True:
                    data = bytes([73, 3, iterations // 256, iterations % 256])
                    start = time.perf_counter()
                    result = subprocess.run([str(root / "base/app")], input=data,
                                            capture_output=True, timeout=30)
                    elapsed = time.perf_counter() - start
                    row = {"case": kind, "snapshot": snapshot, "arm": "base", "depth": 3,
                           "iterations": iterations, "wall_ms": elapsed * 1000,
                           "input": data.hex(), "actual": result.stdout.hex(),
                           "exit": result.returncode, "stderr": result.stderr.hex()}
                    calibration.append(row)
                    append(out / "calibration.jsonl", row)
                    assert result.returncode == 0, row
                    if elapsed > .15 or iterations >= 16384:
                        break
                    iterations *= 2
                for depth in DEPTHS:
                    data = bytes([73, depth, iterations // 256, iterations % 256])
                    want = multi.expected(kind, n, snapshot, data)
                    for rep in range(-1, 7):
                        order = list(ARMS)
                        random.Random(f"{kind}/{snapshot}/{depth}/{rep}").shuffle(order)
                        for arm in order:
                            row, stdout, stderr = measure.timed(
                                [root / arm / "app"], out / "runtime-logs" / str(len(runs)), data, 60)
                            row.update(case=kind, snapshot=snapshot, depth=depth, repetition=rep,
                                       warmup=rep < 0, arm=arm, iterations=iterations,
                                       input=data.hex(), expected=want.hex(), actual=stdout.hex(),
                                       valid=row["exit"] == 0 and not row["timeout"] and stdout == want and not stderr)
                            runs.append(row)
                            measure.dump(Path(row["log"]) / "record.json", row)
                            append(out / "runtime.jsonl", row)
                            assert row["valid"], row
                print(json.dumps({"measured": kind, "snapshot": snapshot,
                                  "iterations": iterations, "requests": len(runs)}), flush=True)
    except Exception as error:
        failures.append({"error": repr(error)})
        raise
    finally:
        quality = []
        for kind, _ in SPECS:
            for snapshot in SNAPSHOTS:
                for depth in DEPTHS:
                    values = {arm: [row["wall_ms"] for row in runs
                                    if (row["case"], row["snapshot"], row["depth"], row["arm"])
                                    == (kind, snapshot, depth, arm)
                                    and row["valid"] and row["repetition"] >= 0] for arm in ARMS}
                    if not all(len(value) == 7 for value in values.values()):
                        continue
                    medians = {arm: statistics.median(value) for arm, value in values.items()}
                    base_rust90 = medians["rust"] / medians["base"] >= .9
                    cand_rust90 = medians["rust"] / medians["cand"] >= .9
                    quality.append({"case": kind, "snapshot": snapshot, "depth": depth,
                                    "median_ms": medians, "values_ms": values,
                                    "candidate_latency_ratio": medians["cand"] / medians["base"],
                                    "delta_pass": medians["cand"] <= 1.1 * medians["base"],
                                    "base_rust90_pass": base_rust90, "rust90_pass": cand_rust90,
                                    "new_rust90_regression": base_rust90 and not cand_rust90})
        complete = (not failures and len(rows) == 24 and all(row["valid"] for row in rows)
                    and len(runs) == 384 and all(row["valid"] for row in runs) and len(quality) == 16)
        summary = {"complete": complete, "compile_requests": len(rows),
                   "native_output_checks": sum(len(row["native_checks"]) for row in rows),
                   "runtime_requests": len(runs), "runtime_formal_requests": sum(not row["warmup"] for row in runs),
                   "calibration_requests": len(calibration), "quality": quality,
                   "quality_complete": len(quality) == 16,
                   "no_new_runtime_regression": complete and all(row["delta_pass"] for row in quality),
                   "rust90_all": complete and all(row["rust90_pass"] for row in quality),
                   "no_new_rust90_crossing": complete and not any(row["new_rust90_regression"] for row in quality),
                   "failures": failures, "weighted_in_original_protocol": False,
                   "acceptance": "Independent exact-source review required; failed gates remain failed"}
        measure.dump(out / "summary.json", summary)
        print(json.dumps({key: value for key, value in summary.items() if key != "quality"}, indent=2), flush=True)


if __name__ == "__main__":
    main()

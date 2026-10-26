#!/usr/bin/env python3
"""Small, stdlib-only reproducible runner. Never runs a ZIL product gate."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import resource
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "tools/research/050"
CASES = ROOT / "tests/perf/impl050/cases.txt"
FLAGS = ["--edition=2024", "-C", "opt-level=3", "-C", "codegen-units=1", "-C", "panic=abort", "-C", "debuginfo=0"]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def identities():
    paths = sorted(SOURCE.glob("*.rs")) + [SOURCE / "run.py", CASES, ROOT / "docs/research/050/CONTRACT.md"]
    return {str(p.relative_to(ROOT)): sha(p) for p in paths}


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def build(binary):
    subprocess.run(["rustc", *FLAGS, "tools/research/050/main.rs", "-o", str(binary)], cwd=ROOT, check=True)


def quantile(values, q):
    values = sorted(values)
    p = (len(values) - 1) * q
    lower = int(p)
    upper = min(lower + 1, len(values) - 1)
    return values[lower] + (values[upper] - values[lower]) * (p - lower)


def summarize(raw, wall, rss):
    groups = {}
    for line in raw.read_text().splitlines():
        row = json.loads(line)
        groups.setdefault((row["case"], row["method"]), []).append(row)
    output = {}
    for (case, method), rows in groups.items():
        assert len(rows) == 42
        assert {(r["round"], r["sample"]) for r in rows} == {(r, s) for r in range(6) for s in range(7)}
        phases = {}
        for name in ["build", "plan", "solve", "check", "teardown", "total"]:
            values = [r[f"batch_{name}_ns"] / r["batch"] / 1_000_000 for r in rows]
            phases[name + "_ms"] = {
                "median": statistics.median(values), "p10": quantile(values, .1),
                "p90": quantile(values, .9), "iqr": quantile(values, .75) - quantile(values, .25),
                "min": min(values), "max": max(values),
            }
        output.setdefault(case, {})[method] = {"samples": len(rows), "batch": rows[0]["batch"], "phases": phases}
    for case, values in output.items():
        values["flat_over_cut_median"] = values["flat"]["phases"]["total_ms"]["median"] / values["cut"]["phases"]["total_ms"]["median"]
    return {"wall_seconds_including_startup_and_output": wall, "whole_child_peak_rss_kib": rss,
            "rss_scope": "entire benchmark child process, not attributed to a case or algorithm",
            "raw_sha256": sha(raw), "cases": output}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["freeze", "bench"])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, default=ROOT / "evidence/impl/050")
    parser.add_argument("--cpu", type=int)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    args.evidence.mkdir(parents=True, exist_ok=True)
    manifest_path = args.evidence / "input-manifest.json"
    binary = args.out / "cut050"
    inputs = args.out / "inputs"
    allowed = sorted(os.sched_getaffinity(0))
    if args.mode == "freeze":
        if manifest_path.exists():
            raise SystemExit("refusing to overwrite an existing freeze; use a new evidence directory")
        cpu = args.cpu if args.cpu is not None else allowed[0]
        if cpu not in allowed:
            raise SystemExit("requested CPU is unavailable")
        os.sched_setaffinity(0, {cpu})
        tracked = subprocess.check_output(["git", "status", "--porcelain", "--", "tools/research/050", "tests/perf/impl050", "docs/research/050/CONTRACT.md"], cwd=ROOT, text=True)
        if tracked.strip():
            raise SystemExit("commit research source and contract before freeze")
        build(binary)
        freeze_lines = subprocess.check_output([str(binary), "freeze", str(CASES), str(inputs)], cwd=ROOT, text=True)
        cases = [json.loads(line) for line in freeze_lines.splitlines()]
        for case in cases:
            name = case["case"]
            case["model_sha256"] = sha(inputs / (name + ".model"))
            case["output_sha256"] = sha(inputs / (name + ".rows"))
        manifest = {"contract": "050/v1", "pre_timing_freeze": True,
                    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    "source_sha256": identities(), "binary_sha256": sha(binary),
                    "rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
                    "rustc_flags": FLAGS, "cpu": cpu, "available_cpus_at_freeze": allowed,
                    "limits": {"max_relation_rows": 131072, "relation_work": 5000000, "max_scope": 24, "oracle_nodes": 5000000},
                    "samples": {"rounds": 6, "samples_per_round": 7, "order": "alternating by round+sample+phase"},
                    "cases": cases}
        write_json(manifest_path, manifest)
        print(f"frozen {len(cases)} cases; all complete relations equal oracle and exact flat baseline; {manifest_path}")
    else:
        manifest = json.loads(manifest_path.read_text())
        if identities() != manifest["source_sha256"]:
            raise SystemExit("source/contract differs from frozen identities")
        if not binary.exists():
            build(binary)
        if sha(binary) != manifest["binary_sha256"]:
            raise SystemExit("binary differs from freeze")
        cpu = manifest["cpu"]
        if cpu not in allowed or (args.cpu is not None and args.cpu != cpu):
            raise SystemExit("frozen CPU unavailable or different")
        os.sched_setaffinity(0, {cpu})
        for case in manifest["cases"]:
            for suffix, key in [("model", "model_sha256"), ("rows", "output_sha256")]:
                if sha(inputs / (case["case"] + "." + suffix)) != case[key]:
                    raise SystemExit("frozen input/output file changed")
        raw = args.evidence / "paired-raw.jsonl"
        if raw.exists():
            raise SystemExit("refusing to overwrite raw samples")
        start = time.monotonic()
        with raw.open("w") as output, (args.evidence / "bench-stderr.log").open("w") as errors:
            subprocess.run([str(binary), "bench", str(CASES), str(inputs)], cwd=ROOT, stdout=output, stderr=errors, check=True)
        wall = time.monotonic() - start
        rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
        summary = summarize(raw, wall, rss)
        summary["frozen_manifest_sha256"] = sha(manifest_path)
        summary["binary_sha256"] = sha(binary)
        summary["cpu"] = cpu
        write_json(args.evidence / "paired-summary.json", summary)
        print(f"measured all 18 frozen cases; wall={wall:.3f}s; {args.evidence / 'paired-summary.json'}")


if __name__ == "__main__":
    main()

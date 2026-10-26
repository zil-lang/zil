#!/usr/bin/env python3
"""Compare two prebuilt identical harnesses; never modifies product gates."""
import argparse
import datetime
import hashlib
import json
import os
import platform
import statistics
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--batches", type=int, default=6)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--case", action="append", help="optional frozen case filename; repeat to select cases")
    parser.add_argument("--cpu", type=int, help="optional single Linux CPU affinity inherited by both versions")
    parser.add_argument("--candidate-revision", help="exact source revision used to build the candidate")
    args = parser.parse_args()
    if args.out.exists():
        parser.error("output exists; preserve earlier measurements with a new output path")
    if args.batches < 1 or not 1 <= args.samples <= 100:
        parser.error("positive batches and 1..100 samples required")
    if args.cpu is not None:
        if args.cpu not in os.sched_getaffinity(0):
            parser.error("CPU is outside current allowed affinity")
        os.sched_setaffinity(0, {args.cpu})
    root = Path(__file__).resolve().parents[1]
    suite = root / "tests/perf/impl046"
    contract = json.loads((suite / "contract.json").read_text())
    selected = set(args.case or (case["file"] for case in contract["cases"]))
    if not selected.issubset({case["file"] for case in contract["cases"]}):
        parser.error("selected case is outside frozen contract")
    started = time.monotonic()
    report = {"contract": contract, "platform": platform.platform(),
              "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "cpu_affinity": sorted(os.sched_getaffinity(0)),
              "candidate_revision": args.candidate_revision,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "harness_sha256": hashlib.sha256((root / "crates/zil-driver/examples/proof_frontier.rs").read_bytes()).hexdigest(),
              "baseline_executable_sha256": hashlib.sha256(args.baseline.read_bytes()).hexdigest(),
              "candidate_executable_sha256": hashlib.sha256(args.candidate.read_bytes()).hexdigest(),
              "batches": args.batches, "samples_per_batch": args.samples, "results": []}
    for case in contract["cases"]:
        if case["file"] not in selected:
            continue
        source = suite / case["file"]
        assert hashlib.sha256(source.read_bytes()).hexdigest() == case["sha256"]
        observations = {"baseline": [], "candidate": []}
        identity = None
        raw = []
        for batch in range(args.batches):
            order = ("baseline", "candidate") if batch % 2 == 0 else ("candidate", "baseline")
            for name in order:
                executable = getattr(args, name).resolve()
                process = subprocess.run([str(executable), str(source), str(args.samples)],
                                         capture_output=True, text=True, timeout=60, check=True)
                observation = json.loads(process.stdout)
                this_identity = (observation["source_sha256"], observation["candidate_sha256"], observation["functions"])
                if identity is None:
                    identity = this_identity
                assert identity == this_identity, (case["file"], "candidate mismatch")
                observations[name].extend(observation["samples_ns"])
                raw.append({"batch": batch, "implementation": name, **observation})
        medians = {name: [statistics.median(row[i] for row in rows) for i in range(3)]
                   for name, rows in observations.items()}
        dispersion = {}
        for name, rows in observations.items():
            dispersion[name] = []
            for i in range(3):
                values = [row[i] for row in rows]
                deciles = statistics.quantiles(values, n=10, method="inclusive") if len(values) > 1 else values * 9
                quartiles = statistics.quantiles(values, n=4, method="inclusive") if len(values) > 1 else values * 3
                dispersion[name].append({"count": len(values), "min": min(values), "max": max(values),
                                         "p10": deciles[0], "median": medians[name][i], "p90": deciles[8],
                                         "iqr": quartiles[2] - quartiles[0]})
        ratios = [medians["baseline"][i] / medians["candidate"][i] for i in range(3)]
        result = {"case": case["file"], "candidate_bytes_equal": True,
                  "median_ns": medians, "distribution_ns": dispersion, "baseline_over_candidate": ratios, "raw": raw}
        report["results"].append(result)
        print(case["file"], "analyze/verify/full speedup:", *(f"{n:.3f}" for n in ratios), flush=True)
    report["status"] = "Pass"
    report["elapsed_seconds"] = time.monotonic() - started
    report["scope"] = "Paired implementation benchmark; no Rust language comparison or product-gate conclusion."
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()

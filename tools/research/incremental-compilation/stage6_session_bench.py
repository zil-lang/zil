#!/usr/bin/env python3
"""Paired complete Session requests; six AB/BA blocks and full native outputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
PROBE = Path(__file__).with_name("stage6_session_probe.rs")
CASES = ["large-512", "large-2048", "even-64x16", "even-512x8",
         "even-1024x4", "branch-64x16", "body-16x16"]
REQUESTS = ["cold", "unchanged", "private", "public", "restore"]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def build_probe(target, output):
    deps = target.resolve() / "release/deps"
    cmd = ["rustc", "--edition=2024", "-O", "-C", "panic=abort", str(PROBE), "-o", str(output),
           "-L", "dependency=" + str(deps), "-L", "native=" + os.environ["ZIL_LLVM_LIB_DIR"]]
    identities = {}
    for crate in ["zil_driver", "zil_base"]:
        libraries = list(deps.glob("lib" + crate + "-*.rlib"))
        assert len(libraries) == 1, (crate, libraries)
        library = libraries[0]
        cmd += ["--extern", crate + "=" + str(library)]
        identities[crate] = {"path": str(library), "sha256": sha(library)}
    subprocess.run(cmd, cwd=ROOT, check=True)
    return {"command": cmd, "libraries": identities, "binary_sha256": sha(output)}

def run(cmd, directory, timeout):
    directory.mkdir(parents=True)
    started = time.perf_counter_ns()
    with (directory / "stdout.jsonl").open("wb") as out, (directory / "stderr").open("wb") as err:
        child = subprocess.Popen(cmd, stdout=out, stderr=err, start_new_session=True)
        try:
            status = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            status = None
    result = {"command": cmd, "exit": status, "process_wall_ns": time.perf_counter_ns() - started}
    (directory / "process.json").write_text(json.dumps(result, indent=2) + "\n")
    assert status == 0, result
    return [json.loads(line) for line in (directory / "stdout.jsonl").read_text().splitlines()]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-target", type=Path, required=True)
    parser.add_argument("--candidate-target", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--case", action="append", choices=CASES)
    parser.add_argument("--mode", action="append", choices=["check", "O0", "O2"])
    parser.add_argument("--blocks", type=int, default=6)
    parser.add_argument("--controls", action="store_true")
    parser.add_argument("--prepare-only", action="store_true")
    parser.add_argument("--timeout", type=int, default=600)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    cases, modes = args.case or CASES, args.mode or ["check", "O0"]
    binaries = {arm: args.out / ("probe-" + arm) for arm in ["baseline", "candidate"]}
    manifest = {"cases": cases, "modes": modes, "blocks": args.blocks, "cpus": cpus,
                "requests": REQUESTS, "probe_sha256": sha(PROBE), "runner_sha256": sha(Path(__file__)),
                "scope": "Complete compile_project calls, 32 raw slots; source writes/native/fresh controls outside timed requests. VmRSS is current process retained memory, not per-request peak. Both arms use default O2 if selected; no incremental_o2 switch in this probe.",
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "binaries": {}}
    for arm, target in [("baseline", args.baseline_target), ("candidate", args.candidate_target)]:
        manifest["binaries"][arm] = build_probe(target, binaries[arm])
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if args.prepare_only:
        return
    rows = []
    source_identities = {}
    if args.controls:
        for case in cases:
            for arm, binary in binaries.items():
                dest = args.out / "controls" / case / arm
                records = run([str(binary), case, "controls", str(dest / "work")], dest, args.timeout)
                assert len(records) == 9, records
    for case in cases:
        for mode in modes:
            for block in range(args.blocks):
                order = ["baseline", "candidate"] if block % 2 == 0 else ["candidate", "baseline"]
                for position, arm in enumerate(order):
                    dest = args.out / f"{case}-{mode}-{block}-{position}-{arm}"
                    records = run([str(binaries[arm]), case, mode, str(dest / "work")], dest, args.timeout)
                    assert [row["request"] for row in records] == REQUESTS
                    for row in records:
                        assert row["report"]["status"] == "Success" and row["report"]["verification_runs"] == 1
                        assert row["native_checked"] == (mode != "check")
                        if mode != "check":
                            assert bytes.fromhex(row["native_stdout_hex"]) == (str(row["expected"]) + "\n").encode()
                        sources = {p.name: sha(p) for p in sorted((dest / "work/sources" / row["request"]).glob("*.zil"))}
                        identity_key = (case, row["request"])
                        assert sources == source_identities.setdefault(identity_key, sources)
                        row.update(case=case, mode=mode, block=block, position=position, arm=arm,
                                   source_sha256=sources, native_stdout_expected=str(row["expected"]) + "\n")
                        rows.append(row)
                        with (args.out / "rows.jsonl").open("a") as output:
                            output.write(json.dumps(row) + "\n")
                print(case, mode, block + 1, "/", args.blocks, flush=True)
    cells = []
    for case in cases:
        for mode in modes:
            for request in REQUESTS:
                values = {}
                for arm in binaries:
                    subset = [r for r in rows if (r["case"], r["mode"], r["request"], r["arm"]) == (case, mode, request, arm)]
                    values[arm] = {"median_ms": statistics.median(r["elapsed_ns"] / 1e6 for r in subset),
                                   "median_rss_kib": statistics.median(r["rss_kib"] for r in subset)}
                cells.append({"case": case, "mode": mode, "request": request, "arms": values,
                              "candidate_over_baseline": values["candidate"]["median_ms"] / values["baseline"]["median_ms"]})
    (args.out / "summary.json").write_text(json.dumps({"rows": len(rows), "cells": cells}, indent=2) + "\n")
    print("complete", len(rows), flush=True)

if __name__ == "__main__":
    main()

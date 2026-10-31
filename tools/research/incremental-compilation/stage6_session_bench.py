#!/usr/bin/env python3
"""Paired complete Session requests; six AB/BA blocks and full native outputs."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import sys
import time
import traceback

ROOT = Path(__file__).resolve().parents[3]
PROBE = Path(__file__).with_name("stage6_session_probe.rs")
CASES = ["large-512", "large-2048", "even-64x16", "even-512x8",
         "even-1024x4", "branch-64x16", "body-16x16"]
REQUESTS = ["cold", "unchanged", "private", "public", "restore"]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def build_probe(target, output):
    deps = target.resolve() / "release/deps"
    log_path = output.with_suffix(".build.log")
    log_path.write_text("Preparing probe from " + str(deps) + "\n")
    cmd = ["rustc", "--edition=2024", "-O", "-C", "panic=abort", str(PROBE), "-o", str(output),
           "-L", "dependency=" + str(deps), "-L", "native=" + os.environ["ZIL_LLVM_LIB_DIR"]]
    identities = {}
    for crate in ["zil_driver", "zil_base"]:
        libraries = list(deps.glob("lib" + crate + "-*.rlib"))
        assert len(libraries) == 1, (crate, libraries)
        library = libraries[0]
        cmd += ["--extern", crate + "=" + str(library)]
        identities[crate] = {"path": str(library), "sha256": sha(library)}
    with log_path.open("a") as log:
        log.write(json.dumps({"command": cmd, "libraries": identities}) + "\n")
        log.flush()
        subprocess.run(cmd, cwd=ROOT, check=True, stdout=log, stderr=subprocess.STDOUT)
    return {"command": cmd, "libraries": identities, "binary_sha256": sha(output),
            "build_log": str(log_path), "build_log_sha256": sha(log_path)}

def run(cmd, directory, timeout):
    directory.mkdir(parents=True)
    started = time.perf_counter_ns()
    launch_error = None
    timed_out = False
    with (directory / "stdout.jsonl").open("wb") as out, (directory / "stderr").open("wb") as err:
        try:
            child = subprocess.Popen(cmd, stdout=out, stderr=err, start_new_session=True)
        except OSError as error:
            launch_error = str(error)
            status = None
        else:
            try:
                status = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                status = None
    result = {"command": cmd, "exit": status, "timeout": timed_out, "launch_error": launch_error,
              "process_wall_ns": time.perf_counter_ns() - started}
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
    execution_mode = parser.add_mutually_exclusive_group()
    execution_mode.add_argument("--prepare-only", action="store_true")
    execution_mode.add_argument("--run-prepared", action="store_true")
    parser.add_argument("--timeout", type=int, default=600)
    args = parser.parse_args()
    args.out = args.out.resolve()
    if args.run_prepared:
        assert args.out.is_dir() and (args.out / "manifest.json").is_file()
        assert not (args.out / "rows.jsonl").exists()
        assert not (args.out / "execution.json").exists()
    else:
        args.out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    cases, modes = args.case or CASES, args.mode or ["check", "O0"]
    binaries = {arm: args.out / ("probe-" + arm) for arm in ["baseline", "candidate"]}
    manifest = {"cases": cases, "modes": modes, "blocks": args.blocks, "cpus": cpus,
                "requests": REQUESTS, "probe_sha256": sha(PROBE), "runner_sha256": sha(Path(__file__)),
                "scope": "Complete compile_project calls, 32 raw slots; source writes/native/fresh controls outside timed requests. VmRSS is current process retained memory, not per-request peak. Both arms use default O2 if selected; no incremental_o2 switch in this probe.",
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "controls": args.controls,
                "targets": {"baseline": str(args.baseline_target.resolve()), "candidate": str(args.candidate_target.resolve())},
                "prepared": False, "binaries": {}}
    if args.run_prepared:
        prepared = json.loads((args.out / "manifest.json").read_text())
        assert prepared["prepared"], "probe preparation did not complete"
        for field in ["cases", "modes", "blocks", "cpus", "requests", "probe_sha256",
                      "runner_sha256", "controls", "targets"]:
            assert prepared[field] == manifest[field], ("prepared configuration changed", field)
        for arm, identity in prepared["binaries"].items():
            assert sha(binaries[arm]) == identity["binary_sha256"], arm
            for library in identity["libraries"].values():
                assert sha(Path(library["path"])) == library["sha256"], library["path"]
        manifest = prepared
    else:
        (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        for arm, target in [("baseline", args.baseline_target), ("candidate", args.candidate_target)]:
            manifest["binaries"][arm] = build_probe(target, binaries[arm])
            (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        manifest["prepared"] = True
        (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    rows = []
    source_identities = {}
    if args.controls:
        marker = args.out / "controls-complete.json"
        if args.run_prepared:
            assert json.loads(marker.read_text()) == {
                "records": len(cases) * 2 * 9, "manifest_sha256": sha(args.out / "manifest.json")}
        else:
            for case in cases:
                for arm, binary in binaries.items():
                    dest = args.out / "controls" / case / arm
                    records = run([str(binary), case, "controls", str(dest / "work")], dest, args.timeout)
                    assert len(records) == 9, records
            marker.write_text(json.dumps({"records": len(cases) * 2 * 9,
                "manifest_sha256": sha(args.out / "manifest.json")}, indent=2) + "\n")
    if args.prepare_only:
        return
    execution = {"started_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                 "command": sys.argv, "manifest_sha256": sha(args.out / "manifest.json"),
                 "precompiled_probes": args.run_prepared, "completed": False}
    (args.out / "execution.json").write_text(json.dumps(execution, indent=2) + "\n")
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
    execution.update(completed=True, finished_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                     rows=len(rows), cells=len(cells))
    (args.out / "execution.json").write_text(json.dumps(execution, indent=2) + "\n")
    print("complete", len(rows), flush=True)

if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        if "--out" in sys.argv:
            output = Path(sys.argv[sys.argv.index("--out") + 1])
            if output.is_dir():
                with (output / "failures.jsonl").open("a") as log:
                    log.write(json.dumps({"time_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        "command": sys.argv, "error_type": type(error).__name__,
                        "error": str(error), "traceback": traceback.format_exc()}) + "\n")
        raise

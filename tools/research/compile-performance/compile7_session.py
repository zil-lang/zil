#!/usr/bin/env python3
"""Freeze, control, then measure paired persistent sessions; Python stdlib only."""
import argparse
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import threading
import time

REQUESTS = ["cold", "unchanged", "private", "public", "restore", "private_again", "restore_again"]
SNAPSHOTS = ["base", "base", "private", "public", "base", "private", "base"]
SCHEMA = "compile7-session-v3"
def sha(path):
    with Path(path).open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()
def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")
def environment():
    # Bind every requested value without persisting possible registry credentials.
    return {k: hashlib.sha256(v.encode()).hexdigest() for k, v in os.environ.items()
            if k == "PATH" or k.startswith(("ZIL_", "LLVM_", "LD_", "RUST", "CARGO_"))}
def host():
    limits = ("cpu.max", "memory.max", "cpu/cpu.cfs_quota_us", "cpu/cpu.cfs_period_us",
              "memory/memory.limit_in_bytes")
    cpu = Path("/proc/cpuinfo").read_text()
    return {"cpu_model": next((line.split(":", 1)[1].strip() for line in cpu.splitlines()
                                if line.startswith("model name")), "unavailable"),
            "quotas": {name: p.read_text().strip() if p.exists() else None for name in limits
                       for p in [Path("/sys/fs/cgroup") / name]}}
def inventory(path):
    path = Path(path).absolute()
    return {"path": str(path), "realpath": str(path.resolve()), "sha256": sha(path)}
def fixtures(root, cases):
    result = {}
    for case, snapshot in itertools.product(cases, ("base", "private", "public")):
        directory = root / case / snapshot / "zil"
        if not (directory / "main.zil").is_file():
            raise ValueError("missing fixture main: " + str(directory))
        for path in sorted(directory.rglob("*")):
            if path.is_symlink():
                raise ValueError("symlink fixture: " + str(path))
            if path.is_file():
                result[str(path.relative_to(root))] = sha(path)
    return result
def freeze(a):
    root, corpus = a.out.resolve(), a.fixtures.resolve()
    cases, capacities = a.cases.split(","), a.capacities.split(",")
    if len(set(cases)) != len(cases) or len(set(capacities)) != len(capacities) or any(Path(c).name != c for c in cases):
        raise ValueError("unique case names and capacities required")
    cpus = list(map(int, a.cpus.split(",")))
    if not cpus or not set(cpus) <= os.sched_getaffinity(0):
        raise ValueError("CPUs not available")
    for capacity in capacities:
        if capacity != "default" and int(capacity) < 8:
            raise ValueError("explicit capacity must be at least 8")
    binaries = {}
    for arm, binary, source in (("A", a.baseline, a.baseline_source), ("B", a.candidate, a.candidate_source)):
        if len(source) != 40 or any(c not in "0123456789abcdef" for c in source):
            raise ValueError("full source commit required")
        binaries[arm] = {"path": str(binary.resolve()), "sha256": sha(binary), "declared_source": source}
    order = [{"block": block, "capacity": cap, "case": case, "position": pos, "arm": arm}
             for block in range(5) for cap in capacities for case in cases
             for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
    plan = {"schema": SCHEMA, "binaries": binaries, "cases": cases,
            "capacities": capacities, "cpus": cpus, "blocks": 5, "timeout": a.timeout,
            "runner_sha256": sha(__file__), "harness": str(a.harness.resolve()),
            "harness_sha256": sha(a.harness), "fixtures": str(corpus),
            "fixtures_sha256": fixtures(corpus, cases), "requests": REQUESTS, "order": order,
            "environment_sha256": environment(), "host": host(), "tools": [inventory(p) for p in a.tool],
            "shared_host": "Fixed local affinity and quotas; quiet-window declares known builds/tests stopped, not dedicated hardware or exclusive OS caches.",
            "scope": "Complete compile_path through Verify, including source discovery; no LLVM emission/link. Copying edit snapshots and session initialization separate. Process wait4 wall/CPU/RSS also retained; no claim of AOT end-to-end speed.",
            "rss_scope": "Linux wait4 maximum single-process RSS in KiB, not summed simultaneous tree RSS",
            "control_scope": "Observable report/diagnostic/interface equality versus fresh session; no native or Typed IR equivalence claim",
            "failure_rule": "Any failed/missing/different observable, changed identity, timeout or failed control suppresses scoring"}
    root.mkdir(parents=True, exist_ok=False)
    write(root / "plan.json", plan)
    print(root / "plan.json")
def validate(plan):
    if plan["schema"] != SCHEMA or sha(__file__) != plan["runner_sha256"]:
        raise ValueError("runner changed")
    if sha(plan["harness"]) != plan["harness_sha256"]:
        raise ValueError("harness changed")
    if any(sha(b["path"]) != b["sha256"] for b in plan["binaries"].values()):
        raise ValueError("binary changed")
    if fixtures(Path(plan["fixtures"]), plan["cases"]) != plan["fixtures_sha256"]:
        raise ValueError("fixtures changed")
    if not set(plan["cpus"]) <= os.sched_getaffinity(0):
        raise ValueError("CPU affinity changed")
    if environment() != plan["environment_sha256"]:
        raise ValueError("compiler environment changed")
    if host() != plan["host"] or any(inventory(t["path"]) != t for t in plan["tools"]):
        raise ValueError("host quota/CPU or frozen tool changed")
def number(value, name, positive=False, integer=False):
    if (type(value) not in ((int,) if integer else (int, float))
            or (type(value) is float and not math.isfinite(value))
            or value < 0 or (positive and value == 0)):
        raise ValueError("invalid numeric field: " + name)
def inspect_output(record, lines, seq, mode):
    requests = [line for line in lines if line.get("kind") == "request"]
    record["requests"] = requests
    record["initialization_ns"] = lines[0]["initialization_ns"]
    number(record["initialization_ns"], "initialization_ns", integer=True)
    for name in ("process_wall_ns", "process_cpu_s", "process_maxrss_kib"):
        number(record[name], name, positive=True, integer=name != "process_cpu_s")
    for request in requests:
        for name in ("compile_ns", "preparation_ns", "report_preparation_ns", "verification_runs",
                     "active_query_count", "cached_query_count"):
            number(request[name], name, positive=name == "compile_ns", integer=True)
    record["pass"] = (record["exit"] == 0 and not record["timeout"] and len(lines) == 9
                      and lines[0]["kind"] == "session"
                      and lines[0]["capacity"] == seq["capacity"] and lines[0]["mode"] == mode
                      and lines[-1] == {"kind": "end", "pass": True}
                      and [r["request"] for r in requests] == REQUESTS
                      and [r["snapshot"] for r in requests] == SNAPSHOTS
                      and all(r["valid"] is True and r["status"] == "Success" and r["phase"] == "verify"
                              and r["verification_runs"] == 1 and r["active_query_count"] == 0
                              and r["phases"].get("verify") == {"executed": 1, "reused": 0}
                              and (mode != "control" or r["fresh_equal"] is True) for r in requests))
def observables(rows):
    signatures = {}
    for row in rows:
        for request in row.get("requests", []):
            key = (row["case"], row["capacity"], request["request"])
            value = request["observable_digest"]
            if key in signatures and signatures[key] != value:
                raise ValueError("observables differ within window")
            signatures[key] = value
    return signatures
def control_order(plan):
    return [{"case": case, "capacity": cap, "arm": arm} for cap in plan["capacities"]
            for case in plan["cases"] for arm in "AB"]
def verify_control(root, plan, plan_path):
    logs = root / "control"
    result = json.loads((logs / "result.json").read_text())
    if not result["pass"] or result["plan_sha256"] != sha(plan_path) or sha(logs / "rows.jsonl") != result["rows_sha256"]:
        raise ValueError("control plan/rows integrity failed")
    rows = [json.loads(line) for line in (logs / "rows.jsonl").read_text().splitlines()]
    expected, manifest = control_order(plan), {}
    if len(rows) != len(expected) or result["rows"] != len(expected):
        raise ValueError("control row count changed")
    for index, (row, seq) in enumerate(zip(rows, expected)):
        if any(row[k] != v for k, v in seq.items()) or not row["pass"]:
            raise ValueError("control sequence/pass changed")
        argv = [plan["binaries"][seq["arm"]]["path"], str(Path(plan["fixtures"]) / seq["case"]),
                str(root / "live"), seq["capacity"], "control"]
        if row["argv"] != argv:
            raise ValueError("control command does not match frozen inputs")
        for kind, suffix in (("stdout", ".stdout.jsonl"), ("stderr", ".stderr.txt")):
            name = f"{index:04d}" + suffix
            if row[kind] != name or sha(logs / name) != row[kind + "_sha256"]:
                raise ValueError("control raw output changed")
            manifest[name] = row[kind + "_sha256"]
        checked = dict(row)
        inspect_output(checked, [json.loads(line) for line in (logs / row["stdout"]).read_text().splitlines()], seq, "control")
        if (not checked["pass"] or checked["requests"] != row["requests"]
                or checked["initialization_ns"] != row["initialization_ns"] or (logs / row["stderr"]).stat().st_size):
            raise ValueError("control raw output does not support success")
    if manifest != result["logs_sha256"]:
        raise ValueError("control log manifest mismatch")
    return sha(logs / "result.json"), observables(rows)
def execute(plan, root, log, seq, mode):
    live = root / "live"
    if live.exists():
        shutil.rmtree(live)
    argv = [plan["binaries"][seq["arm"]]["path"], str(Path(plan["fixtures"]) / seq["case"]),
            str(live), seq["capacity"], mode]
    record = {**seq, "argv": argv, "timeout": False, "pass": False}
    with log.with_suffix(".stdout.jsonl").open("wb") as stdout, log.with_suffix(".stderr.txt").open("wb") as stderr:
        start = time.perf_counter_ns()
        try:
            process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True,
                                       preexec_fn=lambda: os.sched_setaffinity(0, plan["cpus"]))
        except OSError as error:
            record["launch_error"] = str(error)
            return record
        def expired():
            record["timeout"] = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        timer = threading.Timer(plan["timeout"], expired)
        timer.start()
        _, status, usage = os.wait4(process.pid, 0)
        elapsed = time.perf_counter_ns() - start
        timer.cancel()
        process.returncode = os.waitstatus_to_exitcode(status)
        record.update(process_wall_ns=elapsed, process_cpu_s=usage.ru_utime + usage.ru_stime,
                      process_maxrss_kib=usage.ru_maxrss, exit=process.returncode)
    try:
        lines = [json.loads(line) for line in log.with_suffix(".stdout.jsonl").read_text().splitlines()]
        inspect_output(record, lines, seq, mode)
    except (ValueError, KeyError, IndexError, TypeError, AttributeError) as error:
        record["parse_error"] = str(error)
    return record
def run(a):
    path = a.plan.resolve()
    plan, root = json.loads(path.read_text()), path.parent
    plan_digest = sha(path)
    validate(plan)
    mode = "control" if a.control else "measure"
    if not a.control and not a.quiet_window:
        raise ValueError("formal measurement requires --quiet-window; stop known build/test work first")
    control_digest, control_observables = None, None
    if not a.control:
        control_digest, control_observables = verify_control(root, plan, path)
    logdir = root / mode
    logdir.mkdir(exist_ok=False)
    order = control_order(plan) if a.control else plan["order"]
    rows, manifest = [], {}
    with (logdir / "rows.jsonl").open("w") as stream:
        for index, seq in enumerate(order):
            row = execute(plan, root, logdir / f"{index:04d}", seq, mode)
            for kind, suffix in (("stdout", ".stdout.jsonl"), ("stderr", ".stderr.txt")):
                name = f"{index:04d}" + suffix
                row[kind], row[kind + "_sha256"] = name, sha(logdir / name)
                manifest[name] = row[kind + "_sha256"]
            row["pass"] &= (logdir / row["stderr"]).stat().st_size == 0
            rows.append(row)
            stream.write(json.dumps(row) + "\n")
            stream.flush()
    raw_rows = (logdir / "rows.jsonl").read_bytes()
    result = {"plan_sha256": plan_digest, "pass": len(rows) == len(order) and all(r["pass"] for r in rows), "rows": len(rows),
              "rows_sha256": hashlib.sha256(raw_rows).hexdigest(), "logs_sha256": manifest,
              "quiet_window": a.quiet_window, "control_result_sha256": control_digest}
    try:
        if sha(path) != plan_digest:
            raise ValueError("plan changed during window")
        if [json.loads(line) for line in raw_rows.splitlines()] != rows:
            raise ValueError("stored rows differ from execution records")
        validate(plan)
        observed = observables(rows)
        result["pass"] &= all(sha(logdir / name) == digest for name, digest in manifest.items())
        if not a.control:
            if observed != control_observables:
                raise ValueError("measurement observables differ from successful control")
            if verify_control(root, plan, path) != (control_digest, control_observables):
                raise ValueError("control changed during measurement")
    except (ValueError, OSError, KeyError, TypeError, IndexError, AttributeError) as error:
        result.update({"pass": False, "identity_error": str(error)})
    if result["pass"] and not a.control:
        summaries = []
        metrics = REQUESTS + ["sequence", "process_wall", "process_cpu", "process_maxrss"]
        for cap, case, request in itertools.product(plan["capacities"], plan["cases"], metrics):
            pairs = []
            for block in range(5):
                times = {arm: [] for arm in "AB"}
                for row in rows:
                    if (row["capacity"], row["case"], row["block"]) == (cap, case, block):
                        value = (row["process_wall_ns"] / 1e6 if request == "process_wall" else
                                 row["process_cpu_s"] * 1000 if request == "process_cpu" else
                                 row["process_maxrss_kib"] if request == "process_maxrss" else
                                 sum(r["compile_ns"] for r in row["requests"]
                                     if request == "sequence" or r["request"] == request) / 1e6)
                        times[row["arm"]].append(value)
                pairs.append({arm: statistics.mean(values) for arm, values in times.items()})
            ratios = [pair["B"] / pair["A"] for pair in pairs]
            medians = {arm: statistics.median(pair[arm] for pair in pairs) for arm in "AB"}
            summaries.append({"capacity": cap, "case": case, "request": request, "block_values": pairs,
                              "unit": "KiB" if request == "process_maxrss" else "ms",
                              "block_ratios_B_over_A": ratios, "median_ratio": statistics.median(ratios),
                              "median_values": medians, "ratio_of_medians": medians["B"] / medians["A"]})
        result["summaries"] = summaries
    write(logdir / "result.json", result)
    print(logdir / "result.json")
    if not result["pass"]:
        raise SystemExit("Failed window retained; no score")
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze_parser = commands.add_parser("freeze")
    for name in ("baseline", "candidate", "fixtures", "out"):
        freeze_parser.add_argument("--" + name, type=Path, required=True)
    for name in ("baseline-source", "candidate-source"):
        freeze_parser.add_argument("--" + name, required=True)
    freeze_parser.add_argument("--harness", type=Path, default=Path(__file__).with_suffix(".rs"))
    freeze_parser.add_argument("--cases", default="tree4x2,tree32x8,tree256x8,tree1024x8,retained")
    freeze_parser.add_argument("--capacities", default="default,8192")
    freeze_parser.add_argument("--cpus", default=",".join(map(str, sorted(os.sched_getaffinity(0))[:2])))
    freeze_parser.add_argument("--timeout", type=float, default=180.)
    freeze_parser.add_argument("--tool", type=Path, action="append", default=[], help="freeze a loaded library/tool; repeatable")
    run_parser = commands.add_parser("run")
    run_parser.add_argument("plan", type=Path)
    run_parser.add_argument("--control", action="store_true")
    run_parser.add_argument("--quiet-window", action="store_true", help="declare other known build/test work stopped")
    args = parser.parse_args()
    (freeze if args.command == "freeze" else run)(args)
if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Frozen, append-only native object-cache and Rust reference experiments.
No dependency outside Python's standard library. No data is pooled with older runs.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import shutil
import signal
import statistics
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[3]
REQUESTS = {"cold_session": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}
SNAPSHOTS = {"cold_session": "base", "unchanged": "base", "private_edit": "private", "public_edit": "public"}


def sha(path: Path) -> str:
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def write(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def fresh(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=False)
    return path


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def prepare(args: argparse.Namespace) -> None:
    if git("status", "--porcelain"):
        raise SystemExit("Commit the source and runner before freezing a new plan")
    out = fresh(args.out.resolve())
    spec = importlib.util.spec_from_file_location("corpus212", ROOT / "tools/research/210/corpus.py")
    corpus = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(corpus)
    cases = corpus.generate(out)
    cpu_list = sorted(os.sched_getaffinity(0))
    if len(cpu_list) < 4:
        raise SystemExit("Four allowed CPUs required for this frozen comparison")
    blocks = args.blocks or {"primary": 5, "default": 3, "rust": 3, "bridge": 1}[args.mode]
    units = 1 if args.mode in {"default", "bridge"} else 8
    jobs = 1 if units == 1 else 4
    if args.mode == "rust" and args.profile != "O0":
        raise SystemExit("This Rust reference contract is O0 only")
    a = args.rustc if args.mode == "rust" else args.baseline if args.mode == "bridge" else args.candidate
    if a is None:
        raise SystemExit("Selected mode requires --baseline or --rustc")
    binaries = {"A": a.resolve(), "B": args.candidate.resolve(), "clang": args.clang.resolve()}
    order = []
    rng = random.Random(212)
    for block in range(blocks):
        ids = [case["id"] for case in cases]
        rng.shuffle(ids)
        for case in ids:
            for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"):
                order.append({"block": block, "case": case, "position": position, "arm": arm})
    request_weights = {"cold_session": 1.0} if args.mode == "bridge" else REQUESTS
    plan = {
        "schema": "zil-object-cache212/1", "mode": args.mode, "profile": args.profile,
        "created_unix": time.time(), "source_commit": git("rev-parse", "HEAD"),
        "crates_tree": git("rev-parse", "HEAD:crates"), "blocks": blocks,
        "units": units, "jobs": jobs, "cpus": cpu_list[:4],
        "cgroup_cpu_max": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        "cgroup_memory_max": Path("/sys/fs/cgroup/memory.max").read_text().strip(),
        "runner_sha256": sha(Path(__file__)),
        "binaries": {k: {"path": str(v), "sha256": sha(v), "size": v.stat().st_size,
                         "mtime_ns": v.stat().st_mtime_ns, "ctime_ns": v.stat().st_ctime_ns}
                     for k, v in binaries.items()},
        "environment": {k: os.environ.get(k, "") for k in ["PATH", "LD_LIBRARY_PATH", "ZIL_CLANG", "LD_PRELOAD", "LD_AUDIT", "RUSTFLAGS"]},
        "request_weights": request_weights, "cases": cases, "order": order,
        "expected_builds": len(order) * len(request_weights),
        "fixtures_sha256": {str(p.relative_to(out)): sha(p) for p in sorted((out / "corpus").rglob("*")) if p.is_file()},
        "scope": "full CLI build, including current verification/key/cache/link; no build-system no-op; compiler cache cold, OS pages not flushed",
        "positive_behavior_checks": "independent inherited Python oracle outside measured build clock",
        "failure_rule": "retain every row; do not aggregate or renormalize incomplete/failed cells",
    }
    write(out / "plan.json", plan)
    print(out / "plan.json")


def load(path: Path) -> tuple[dict, Path]:
    path = path.resolve()
    plan = json.loads(path.read_text())
    if sha(Path(__file__)) != plan["runner_sha256"]:
        raise SystemExit("Runner changed after freeze; retain this plan and create another")
    for spec in plan["binaries"].values():
        p = Path(spec["path"]); stat = p.stat()
        if (sha(p), stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns) != (spec["sha256"], spec["size"], spec["mtime_ns"], spec["ctime_ns"]):
            raise SystemExit("Frozen tool or installed-tool metadata changed")
    for name, digest in plan["fixtures_sha256"].items():
        if sha(path.parent / name) != digest:
            raise SystemExit("Frozen source/oracle changed: " + name)
    if any(os.environ.get(k, "") != v for k, v in plan["environment"].items()):
        raise SystemExit("Frozen environment changed")
    return plan, path.parent


def sync_sources(src: Path, dest: Path) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    names = {p.name for p in src.iterdir() if p.is_file()}
    for p in dest.iterdir():
        if p.is_file() and p.name not in names:
            p.unlink()
    for p in src.iterdir():
        if p.is_file():
            target = dest / p.name
            data = p.read_bytes()
            if not target.exists() or target.read_bytes() != data:
                target.write_bytes(data)


def timed(argv: list[str], stdout: Path, stderr: Path, cpus: list[int], timeout: int = 180) -> dict:
    expired = threading.Event()
    def affinity() -> None:
        os.sched_setaffinity(0, cpus)
    with stdout.open("wb") as out, stderr.open("wb") as err:
        start = time.perf_counter_ns()
        child = subprocess.Popen(argv, stdout=out, stderr=err, preexec_fn=affinity, start_new_session=True)
        def kill() -> None:
            expired.set()
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        watchdog = threading.Timer(timeout, kill)
        watchdog.start()
        _, status, usage = os.wait4(child.pid, 0)
        stop = time.perf_counter_ns()
        watchdog.cancel(); watchdog.join()
        child.returncode = os.waitstatus_to_exitcode(status)
    return {"argv": argv, "wall_seconds": (stop - start) / 1e9, "exit": child.returncode,
            "timeout": expired.is_set(), "cpu_user": usage.ru_utime, "cpu_system": usage.ru_stime,
            "max_process_rss_kib": usage.ru_maxrss,
            "memory_note": "wait4 maximum, NOT simultaneous whole-process-tree peak"}


def one_build(plan: dict, root: Path, case: dict, arm: str, request: str, work: Path, logs: Path, tag: str) -> dict:
    rust = plan["mode"] == "rust" and arm == "A"
    language = "rust" if rust else "zil"
    snapshot = SNAPSHOTS[request]
    sync_sources(root / case["root"] / snapshot / language, work / "src")
    out = work / "program"
    for p in [out, out.with_name("program.ll")]:
        p.unlink(missing_ok=True)
    if rust:
        argv = [plan["binaries"][arm]["path"], str(work / "src/main.rs"), "--edition=2024", "--crate-name", "zil_ref212",
                "-Awarnings", "--target", "x86_64-unknown-linux-gnu", "-C", "target-cpu=x86-64",
                "-C", "opt-level=0", "-C", "debuginfo=0", "-C", "overflow-checks=on", "-C", "panic=abort",
                "-C", f"codegen-units={plan['units']}", "-C", "incremental=" + str(work / "rust-incremental"), "-o", str(out)]
    else:
        argv = [plan["binaries"][arm]["path"], "build", str(work / "src/main.zil"), "-o", str(out), "--json",
                "--cache-dir", str(work / "source-candidates"), "--clang", plan["binaries"]["clang"]["path"],
                "--opt", plan["profile"], "--codegen-units", str(plan["units"]), "--codegen-jobs", str(plan["jobs"])]
        if arm == "B" and plan["mode"] != "bridge":
            argv += ["--object-cache", str(work / "objects")]
    stdout = logs / (tag + ".stdout.json"); stderr = logs / (tag + ".stderr.txt")
    record = timed(argv, stdout, stderr, plan["cpus"])
    record.update(case=case["id"], arm=arm, request=request, snapshot=snapshot, language=language,
                  stdout=stdout.name, stderr=stderr.name, checks=[])
    if not rust:
        try:
            report = json.loads(stdout.read_text())
            record.update(status=report.get("status"), verification_runs=report.get("verification_runs"),
                          native_codegen=report.get("native_codegen"), commands=report.get("commands"), disk_cache=report.get("disk_cache"))
        except (OSError, ValueError) as exc:
            record["parse_error"] = repr(exc)
    else:
        record["status"] = "Success" if record["exit"] == 0 else "CompileFailure"
    if record["exit"] == 0 and out.is_file():
        for sample in case["inputs"]:
            try:
                actual = subprocess.run([str(out)], input=(root / sample["path"]).read_bytes(), capture_output=True,
                                        timeout=30, preexec_fn=lambda: os.sched_setaffinity(0, plan["cpus"]))
                expected = (root / sample["expected"][snapshot]).read_bytes()
                record["checks"].append({"input_bytes": sample["bytes"], "exit": actual.returncode,
                    "matches": actual.returncode == 0 and actual.stdout == expected and not actual.stderr,
                    "stdout_sha256": hashlib.sha256(actual.stdout).hexdigest(), "stderr_sha256": hashlib.sha256(actual.stderr).hexdigest()})
            except subprocess.TimeoutExpired:
                record["checks"].append({"input_bytes": sample["bytes"], "matches": False, "timeout": True})
        record["executable_sha256"] = sha(out); record["executable_bytes"] = out.stat().st_size
        if not rust:
            record["llvm_sha256"] = sha(work / "program.ll")
    record["pass"] = (not record["timeout"] and record["exit"] == 0 and record["status"] == "Success"
                      and (rust or record.get("verification_runs") == 1)
                      and len(record["checks"]) == len(case["inputs"]) and all(c["matches"] for c in record["checks"]))
    if not rust and arm == "B" and plan["mode"] != "bridge" and record["pass"]:
        gen = record["native_codegen"]; cache = gen["object_cache"]
        # Key infrastructure must work, not silently bypass a requested experiment.
        record["cache_contract"] = (cache["enabled"] and (request != "unchanged" or cache["hits"] == gen["actual_units"])
                                    and (request != "cold_session" or cache["hits"] == 0))
        record["pass"] &= record["cache_contract"]
    return record


def preflight(args: argparse.Namespace) -> None:
    plan, root = load(args.plan); out = fresh(args.out.resolve()); logs = fresh(out / "logs")
    records = []
    with (out / "records.jsonl").open("x") as stream:
        for case in plan["cases"]:
            for arm in "AB":
                work = fresh(out / f"{case['id']}-{arm}")
                for request in plan["request_weights"]:
                    rec = one_build(plan, root, case, arm, request, work, logs, f"{case['id']}-{arm}-{request}")
                    records.append(rec); stream.write(json.dumps(rec) + "\n"); stream.flush()
                    print(case["id"], arm, request, rec["pass"], round(rec["wall_seconds"], 4), flush=True)
    write(out / "result.json", {"plan_sha256": sha(args.plan), "builds": len(records), "pass": all(r["pass"] for r in records)})


def measure(args: argparse.Namespace) -> None:
    plan, root = load(args.plan); pre = json.loads(args.preflight.read_text())
    if not pre.get("pass") or pre.get("plan_sha256") != sha(args.plan) or not args.quiet_window:
        raise SystemExit("Matching passing preflight and explicit quiet window required")
    out = fresh(args.out.resolve()); logs = fresh(out / "logs"); byid = {c["id"]: c for c in plan["cases"]}
    index = 0
    with (out / "records.jsonl").open("x") as stream:
        for seq in plan["order"]:
            work = fresh(out / f"b{seq['block']}-{seq['case']}-p{seq['position']}-{seq['arm']}")
            for request in plan["request_weights"]:
                rec = one_build(plan, root, byid[seq["case"]], seq["arm"], request, work, logs, f"{index:04d}")
                rec.update(block=seq["block"], position=seq["position"], index=index)
                stream.write(json.dumps(rec) + "\n"); stream.flush(); index += 1
                print(index, plan["expected_builds"], seq["case"], seq["arm"], request, rec["pass"], round(rec["wall_seconds"], 4), flush=True)
    write(out / "complete.json", {"plan_sha256": sha(args.plan), "rows": index, "records_sha256": sha(out / "records.jsonl")})


def aggregate(plan: dict, rows: list[dict], blocks: list[int], metric: str) -> dict:
    cells = {}
    for row in rows:
        value = row["cpu_user"] + row["cpu_system"] if metric == "cpu_seconds" else row[metric]
        cells.setdefault((row["block"], row["case"], row["request"], row["arm"]), []).append(value)
    totals = {"A": 0.0, "B": 0.0}; groups = {}; requests = {r: {"A": 0.0, "B": 0.0} for r in plan["request_weights"]}
    for case in plan["cases"]:
        group = {"A": 0.0, "B": 0.0}
        for request, weight in plan["request_weights"].items():
            for arm in "AB":
                value = statistics.median(statistics.mean(cells[(block, case["id"], request, arm)]) for block in blocks)
                group[arm] += weight * value
                requests[request][arm] += case["weight"] * value
                totals[arm] += case["weight"] * weight * value
        group["B_over_A"] = group["B"] / group["A"]; groups[case["id"]] = group
    for group in requests.values():
        group["B_over_A"] = group["B"] / group["A"]
    return {**totals, "B_over_A": totals["B"] / totals["A"], "by_case": groups, "by_request": requests}


def summarize(args: argparse.Namespace) -> None:
    plan, _ = load(args.plan); rows = [json.loads(line) for line in args.records.read_text().splitlines()]
    expected = [(o["block"], o["case"], o["position"], o["arm"], req) for o in plan["order"] for req in plan["request_weights"]]
    observed = [(r["block"], r["case"], r["position"], r["arm"], r["request"]) for r in rows]
    failures = [r["index"] for r in rows if not r["pass"]]
    result = {"plan_sha256": sha(args.plan), "source_commit": plan["source_commit"], "mode": plan["mode"], "profile": plan["profile"],
              "units": plan["units"], "jobs": plan["jobs"], "rows": len(rows), "coverage_complete": observed == expected,
              "failures": failures, "native_checks": sum(len(r["checks"]) for r in rows), "blocks": plan["blocks"]}
    if failures or observed != expected:
        result["verdict"] = "IncompleteOrFailed_NoScore"; write(args.out, result); raise SystemExit("Do not score missing/failed cells")
    blocks = list(range(plan["blocks"]))
    result["wall"] = aggregate(plan, rows, blocks, "wall_seconds")
    result["cpu"] = aggregate(plan, rows, blocks, "cpu_seconds")
    rng = random.Random(212)
    samples = [aggregate(plan, rows, rng.choices(blocks, k=len(blocks)), "wall_seconds")["B_over_A"] for _ in range(5000)]
    samples.sort(); result["exploratory_paired_block_bootstrap95"] = [samples[124], samples[4874]]
    result["cache_by_request"] = {}
    for request in plan["request_weights"]:
        sub = [r["native_codegen"]["object_cache"] for r in rows if r["arm"] == "B" and r["request"] == request and r.get("native_codegen")]
        result["cache_by_request"][request] = {k: sum(r[k] for r in sub) for k in ["hits", "misses", "corrupt", "stored", "write_failures"]}
    if plan["mode"] != "rust":
        binary_sets = {}
        for row in rows:
            binary_sets.setdefault((row["case"], row["snapshot"]), set()).add(row["executable_sha256"])
        result["same_binary_for_each_case_snapshot"] = all(len(s) == 1 for s in binary_sets.values())
        result["binary_comparison_cells"] = len(binary_sets)
    result["scope"] = plan["scope"]
    result["product_gates"] = "NotRun"; result["independent_reviewer"] = "NotRun"
    write(args.out, result); print(json.dumps(result, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare"); p.add_argument("--out", type=Path, required=True); p.add_argument("--mode", choices=["primary","default","rust","bridge"], required=True)
    p.add_argument("--candidate",type=Path,required=True); p.add_argument("--baseline",type=Path); p.add_argument("--rustc",type=Path); p.add_argument("--clang",type=Path,required=True)
    p.add_argument("--profile",choices=["O0","O2"],default="O0");p.add_argument("--blocks",type=int);p.set_defaults(run=prepare)
    p=sub.add_parser("preflight");p.add_argument("--plan",type=Path,required=True);p.add_argument("--out",type=Path,required=True);p.set_defaults(run=preflight)
    p=sub.add_parser("measure");p.add_argument("--plan",type=Path,required=True);p.add_argument("--out",type=Path,required=True);p.add_argument("--preflight",type=Path,required=True);p.add_argument("--quiet-window",action="store_true");p.set_defaults(run=measure)
    p=sub.add_parser("summarize");p.add_argument("--plan",type=Path,required=True);p.add_argument("--records",type=Path,required=True);p.add_argument("--out",type=Path,required=True);p.set_defaults(run=summarize)
    args=parser.parse_args();args.run(args)
if __name__=="__main__":main()

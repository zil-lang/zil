#!/usr/bin/env python3
"""Frozen A/B full-CLI comparison; Python standard library, Linux only.

The 210 corpus/oracle and 212 timing/aggregation helpers are reused by import.
Every CLI invocation is a new process/session, including unchanged requests.
This measures complete requests, not persistent-session invalidation in isolation.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
SCHEMA = "zil-compile-performance/2"
REQUESTS = {"cold_process": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}
SNAPSHOTS = {"cold_process": "base", "unchanged": "base", "private_edit": "private", "public_edit": "public"}
DEPENDENCIES = ["tools/research/210/corpus.py", "tools/research/212/bench.py",
                "tools/product/109/reference104/run.py", "tools/product/109/reference104/contract.json"]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


legacy = module("bench212", ROOT / DEPENDENCIES[1])
sha, fresh = legacy.sha, legacy.fresh


def save(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def environment():
    names = {"PATH", "LANG", "TMPDIR", "TMP", "TEMP"}
    names.update(k for k in os.environ if k.startswith(("ZIL_", "LLVM_", "LD_", "RUST", "CARGO_", "LC_")))
    return {k: os.environ.get(k, "") for k in sorted(names)}


def inventory(path):
    path = Path(path).resolve(strict=True)
    stat = path.stat()
    return {"path": str(path), "sha256": sha(path), "size": stat.st_size,
            "mtime_ns": stat.st_mtime_ns, "ctime_ns": stat.st_ctime_ns}


def capture(argv):
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
        return {"argv": argv, "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"argv": argv, "error": repr(error)}


def source_identity(ref):
    commit = git("rev-parse", "--verify", ref + "^{commit}")
    return {"commit": commit, "crates_tree": git("rev-parse", commit + ":crates")}


def file_hashes(path):
    return {str(p.relative_to(path)): sha(p) for p in sorted(path.rglob("*")) if p.is_file()}


def host():
    def read(path):
        p = Path(path)
        return p.read_text().strip() if p.is_file() else None
    return {"platform": platform.platform(), "machine": platform.machine(), "python": sys.version,
            "allowed_cpus": sorted(os.sched_getaffinity(0)),
            "cpu_max": read("/sys/fs/cgroup/cpu.max"), "memory_max": read("/sys/fs/cgroup/memory.max"),
            "cpu_model": next((x.split(":", 1)[1].strip() for x in (read("/proc/cpuinfo") or "").splitlines()
                               if x.startswith("model name")), None)}


def prepare(args):
    if sys.platform != "linux" or not hasattr(os, "wait4"):
        raise SystemExit("The inherited affinity/wait4 measurement contract requires Linux")
    if git("status", "--porcelain"):
        raise SystemExit("Commit source and runner before prepare; preserve any previous plan")
    allowed = sorted(os.sched_getaffinity(0))
    cpus = sorted(set(map(int, args.cpus.split(",")))) if args.cpus else allowed[:min(4, len(allowed))]
    if not cpus or not set(cpus) <= set(allowed):
        raise SystemExit("--cpus must be a nonempty subset of the allowed affinity")
    identities = {"A": source_identity(args.baseline_source), "B": source_identity(args.candidate_source)}
    binaries = {"A": inventory(args.baseline), "B": inventory(args.candidate), "clang": inventory(args.clang)}
    # Extra tools explicitly bind loaded LLVM/linker files without guessing dlopen paths.
    extra = [inventory(p) for p in args.tool]
    out = fresh(args.out.resolve())
    corpus = module("corpus210", ROOT / DEPENDENCIES[0])
    cases = corpus.generate(out)
    selected = set(args.case or [c["id"] for c in cases])
    if selected - {c["id"] for c in cases}:
        raise SystemExit("Unknown case; generated fixtures are retained in " + str(out))
    cases = [c for c in cases if c["id"] in selected]
    weight_sum = sum(c["weight"] for c in cases)
    for case in cases:
        case["original_weight"] = case["weight"]
        case["weight"] /= weight_sum
    phases = list(dict.fromkeys(args.phase or ["check", "build"]))
    rng, order = random.Random(args.seed), []
    for block in range(args.blocks):
        pairs = [(phase, case["id"]) for phase in phases for case in cases]
        rng.shuffle(pairs)
        for phase, case in pairs:
            for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"):
                order.append({"block": block, "phase": phase, "case": case, "position": position, "arm": arm})
    dependencies = list(DEPENDENCIES)
    dependencies += [str(p.relative_to(ROOT)) for p in sorted(corpus.CONSUMER.rglob("*")) if p.is_file()]
    versions = {k: capture([v["path"], "--version"]) for k, v in binaries.items()}
    for executable, flags in (("rustc", ["-Vv"]), ("cargo", ["--version"])):
        found = shutil.which(executable)
        versions[executable] = capture([found, *flags]) if found else {"error": "not on PATH"}
    plan = {"schema": SCHEMA, "created_unix": time.time(), "runner_source_commit": git("rev-parse", "HEAD"),
            "source_identity": identities, "source_binding": "caller-declared source commits; binary hashes frozen independently",
            "runner_sha256": sha(Path(__file__)), "dependencies_sha256": {p: sha(ROOT / p) for p in dependencies},
            "binaries": binaries, "extra_tools": extra, "versions": versions, "host": host(),
            "environment": environment(), "cpus": cpus, "blocks": args.blocks, "seed": args.seed,
            "profile": args.profile, "units": args.units, "jobs": args.jobs, "object_cache": args.object_cache,
            "timeout_seconds": args.timeout, "native_timeout_seconds": 30, "phases": phases,
            "selected_cases": [c["id"] for c in cases], "selected_original_weight": weight_sum,
            "selection_note": "Selected case weights normalized once before measurement; phases never pooled",
            "cases": cases, "request_weights": REQUESTS, "order": order,
            "expected_rows": len(order) * len(REQUESTS), "fixtures_sha256": file_hashes(out / "corpus"),
            "scope": "Full CLI new process per request; source candidates reused within each four-request sequence; OS page cache not flushed; not a persistent-session benchmark",
            "cpu_accounting": "Linux wait4 usage of CLI and descendants reaped by it; no external daemons/outliving descendants",
            "memory_accounting": "wait4 max-process RSS, not simultaneous process-tree memory peak",
            "failure_rule": "Preserve all rows/logs; any missing, failed, timed-out or changed-input row suppresses the score",
            "retention": "Frozen fixtures, raw CLI/native stdout/stderr and source/output hashes retained; isolated copied source/cache/executable removed after each four-request sequence",
            "bootstrap_rule": "Resample whole paired blocks; descriptive/exploratory 95% interval, shared host, 3 or 5 blocks",
            "product_gates": "NotRun"}
    save(out / "plan.json", plan)
    print(out / "plan.json")


def load(path, check_environment=True):
    path = path.resolve()
    plan = json.loads(path.read_text())
    if plan["schema"] != SCHEMA or sha(Path(__file__)) != plan["runner_sha256"]:
        raise SystemExit("Runner/schema changed after freeze; retain the old plan")
    for name, digest in plan["dependencies_sha256"].items():
        if sha(ROOT / name) != digest:
            raise SystemExit("Frozen helper/oracle changed: " + name)
    for spec in [*plan["binaries"].values(), *plan["extra_tools"]]:
        if inventory(spec["path"]) != spec:
            raise SystemExit("Frozen executable/library identity changed: " + spec["path"])
    if file_hashes(path.parent / "corpus") != plan["fixtures_sha256"]:
        raise SystemExit("Frozen fixture set changed")
    if check_environment and (environment() != plan["environment"] or not set(plan["cpus"]) <= os.sched_getaffinity(0)):
        raise SystemExit("Frozen environment or allowed CPU set changed")
    if check_environment:
        now = host()
        if any(now[k] != plan["host"][k] for k in ("cpu_max", "memory_max", "cpu_model")):
            raise SystemExit("Frozen host quota or CPU model changed")
    return plan, path.parent


def command(plan, seq, work):
    program = work / "program"
    argv = [plan["binaries"][seq["arm"]]["path"], seq["phase"], str(work / "src/main.zil"), "--json",
            "--cache-dir", str(work / "source-candidates"), "--clang", plan["binaries"]["clang"]["path"],
            "--opt", plan["profile"], "--codegen-units", str(plan["units"]), "--codegen-jobs", str(plan["jobs"])]
    if seq["phase"] == "build":
        argv += ["-o", str(program)]
        if plan["object_cache"]:
            argv += ["--object-cache", str(work / "objects")]
    return argv


def source_hashes(plan, case, snapshot):
    prefix = str(Path(case["root"]).relative_to("corpus") / snapshot / "zil") + "/"
    return {name[len(prefix):]: digest for name, digest in plan["fixtures_sha256"].items()
            if name.startswith(prefix)}


def fixture_sha(plan, path):
    return plan["fixtures_sha256"][str(Path(path).relative_to("corpus"))]


def one(plan, root, case, seq, request, work, logs, tag):
    snapshot = SNAPSHOTS[request]
    legacy.sync_sources(root / case["root"] / snapshot / "zil", work / "src")
    before = file_hashes(work / "src")
    program = work / "program"
    program.unlink(missing_ok=True)
    program.with_suffix(".ll").unlink(missing_ok=True)
    argv = command(plan, seq, work)
    stdout, stderr = logs / (tag + ".stdout.json"), logs / (tag + ".stderr.txt")
    record = {**seq, "request": request, "snapshot": snapshot, "argv": argv,
              "work": str(work), "stdout": stdout.name, "stderr": stderr.name,
              "source_before": before, "checks": [], "pass": False}
    try:
        record.update(legacy.timed(argv, stdout, stderr, plan["cpus"], plan["timeout_seconds"]))
        record["source_after"] = file_hashes(work / "src")
        report = json.loads(stdout.read_text())
        for field in ("status", "phase", "verification_runs", "native_codegen", "commands", "disk_cache", "diagnostics"):
            record["reported_" + field] = report.get(field)
        record["pass"] = (record["exit"] == 0 and not record["timeout"] and report.get("status") == "Success"
                          and report.get("verification_runs") == 1
                          and bool(before) and before == record["source_after"] == source_hashes(plan, case, snapshot))
        if seq["phase"] == "build" and record["pass"]:
            if plan["object_cache"]:
                native = report["native_codegen"]
                cache = native["object_cache"]
                record["cache_contract"] = (cache["enabled"]
                    and (request != "cold_process" or cache["hits"] == 0)
                    and (request != "unchanged" or cache["hits"] == native["actual_units"]))
                record["pass"] &= record["cache_contract"]
            record["executable_sha256"] = sha(program)
            record["executable_bytes"] = program.stat().st_size
            record["llvm_sha256"] = sha(program.with_suffix(".ll"))
            for i, sample in enumerate(case["inputs"]):
                actual_out, actual_err = logs / f"{tag}.run{i}.stdout.bin", logs / f"{tag}.run{i}.stderr.bin"
                payload = (root / sample["path"]).read_bytes()
                try:
                    result = subprocess.run([str(program)], input=payload, capture_output=True,
                                            timeout=plan["native_timeout_seconds"],
                                            preexec_fn=lambda: os.sched_setaffinity(0, plan["cpus"]))
                    actual_out.write_bytes(result.stdout); actual_err.write_bytes(result.stderr)
                    expected = (root / sample["expected"][snapshot]).read_bytes()
                    check = {"exit": result.returncode, "timeout": False,
                             "matches": result.returncode == 0 and result.stdout == expected and not result.stderr}
                except subprocess.TimeoutExpired as error:
                    actual_out.write_bytes(error.stdout or b""); actual_err.write_bytes(error.stderr or b"")
                    check = {"matches": False, "timeout": True}
                check.update(input=sample["path"], expected=sample["expected"][snapshot],
                             input_sha256=hashlib.sha256(payload).hexdigest(),
                             input_after_sha256=sha(root / sample["path"]),
                             expected_stdout_sha256=sha(root / sample["expected"][snapshot]),
                             stdout=actual_out.name, stderr=actual_err.name,
                             stdout_sha256=sha(actual_out), stderr_sha256=sha(actual_err))
                record["checks"].append(check)
            record["pass"] &= len(record["checks"]) == len(case["inputs"]) and all(c["matches"] for c in record["checks"])
            record["executable_after_sha256"] = sha(program)
            record["pass"] &= record["executable_sha256"] == record["executable_after_sha256"]
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        record.update(error=repr(error), **{"pass": False})
    finally:
        for name, path in (("stdout", stdout), ("stderr", stderr)):
            if path.is_file():
                record[name + "_sha256"] = sha(path)
    return record


def execute(plan, root, out, order):
    logs = fresh(out / "logs")
    cases = {c["id"]: c for c in plan["cases"]}
    count, failures = 0, []
    plan_hash = sha(root / "plan.json")
    with (out / "records.jsonl").open("x") as stream:
        for sequence, seq in enumerate(order):
            work = fresh(out / f"s{sequence:04d}-{seq['case']}-{seq['phase']}-{seq['arm']}")
            for request in REQUESTS:
                try:
                    record = one(plan, root, cases[seq["case"]], seq, request, work, logs, f"{count:05d}")
                except Exception as error:
                    record = {**seq, "request": request, "checks": [], "pass": False,
                              "runner_error": repr(error), "snapshot": SNAPSHOTS[request]}
                record.update(index=count, plan_sha256=plan_hash)
                stream.write(json.dumps(record) + "\n"); stream.flush()
                if not record["pass"]:
                    failures.append(count)
                count += 1
                print(count, seq["phase"], seq["case"], seq["arm"], request, record["pass"], flush=True)
            # Only this freshly created sequence directory is disposable. Logs,
            # frozen source/oracle bytes and the manifest remain outside it.
            shutil.rmtree(work)
    return {"plan_sha256": sha(root / "plan.json"), "rows": count, "failures": failures,
            "pass": not failures, "records_sha256": sha(out / "records.jsonl")}


def preflight_order(plan):
    return [{"phase": phase, "case": case["id"], "arm": arm, "position": position, "block": 0}
            for phase in plan["phases"] for case in plan["cases"] for position, arm in enumerate("AB")]


def log_ok(logs, row):
    return all(Path(row[name]).name == row[name] and sha(logs / row[name]) == row[name + "_sha256"]
               for name in ("stdout", "stderr"))


def validate_rows(plan, root, records, rows, order):
    keys = ("block", "phase", "case", "position", "arm", "request")
    expected = [tuple({**seq, "request": request}[key] for key in keys) for seq in order for request in REQUESTS]
    observed = [tuple(row.get(key) for key in keys) for row in rows]
    cases, failures, logs = {c["id"]: c for c in plan["cases"]}, [], records.parent / "logs"
    plan_hash = sha(root / "plan.json")
    for index, row in enumerate(rows):
        try:
            case, snapshot = cases[row["case"]], SNAPSHOTS[row["request"]]
            before = source_hashes(plan, case, snapshot)
            report = json.loads((logs / row["stdout"]).read_text())
            ok = (row["pass"] is True and row["exit"] == 0 and row["timeout"] is False
                  and row["index"] == index and row["plan_sha256"] == plan_hash
                  and row["snapshot"] == snapshot and row["argv"] == command(plan, row, Path(row["work"]))
                  and bool(before) and row["source_before"] == row["source_after"] == before
                  and all(type(row.get(k)) in (int, float) and math.isfinite(row[k]) and row[k] >= 0
                          for k in ("wall_seconds", "cpu_user", "cpu_system")) and row["wall_seconds"] > 0
                  and log_ok(logs, row) and report.get("status") == "Success" and report.get("verification_runs") == 1
                  and all(row["reported_" + name] == report.get(name) for name in
                          ("status", "phase", "verification_runs", "native_codegen", "commands", "disk_cache", "diagnostics")))
            checks = row["checks"]
            if row["phase"] == "build":
                ok &= (len(checks) == len(case["inputs"]) and row["executable_bytes"] > 0
                       and row["executable_sha256"] == row["executable_after_sha256"]
                       and all(isinstance(row[k], str) and len(row[k]) == 64
                               and all(c in "0123456789abcdef" for c in row[k])
                               for k in ("executable_sha256", "llvm_sha256")))
                if plan["object_cache"]:
                    native = report["native_codegen"]
                    cache = native["object_cache"]
                    ok &= (cache["enabled"] is True and row["cache_contract"] is True
                           and (row["request"] != "cold_process" or cache["hits"] == 0)
                           and (row["request"] != "unchanged" or cache["hits"] == native["actual_units"]))
                for check, sample in zip(checks, case["inputs"]):
                    expected_path = sample["expected"][snapshot]
                    ok &= (check["matches"] is True and check["exit"] == 0 and check["timeout"] is False
                           and check["input"] == sample["path"] and check["expected"] == expected_path
                           and check["input_sha256"] == check["input_after_sha256"] == fixture_sha(plan, sample["path"])
                           and check["stdout_sha256"] == check["expected_stdout_sha256"] == fixture_sha(plan, expected_path)
                           and log_ok(logs, check) and (logs / check["stderr"]).stat().st_size == 0
                           and (logs / check["stdout"]).read_bytes() == (root / expected_path).read_bytes())
            else:
                ok &= checks == []
        except (OSError, KeyError, ValueError, TypeError):
            ok = False
        if not ok:
            failures.append(index)
    return observed == expected, failures


def checked_preflight(plan, root, path):
    try:
        receipt, records = json.loads(path.read_text()), path.parent / "records.jsonl"
        rows = [json.loads(line) for line in records.read_text().splitlines()]
        complete, failures = validate_rows(plan, root, records, rows, preflight_order(plan))
        ok = (receipt["pass"] is True and not receipt["failures"] and complete and not failures
              and receipt["plan_sha256"] == sha(root / "plan.json") and receipt["rows"] == len(rows)
              and receipt["records_sha256"] == sha(records))
    except (OSError, KeyError, ValueError, TypeError):
        ok = False
    if not ok:
        raise SystemExit("Matching complete passing raw preflight, sources and logs are required")


def preflight(args):
    plan, root = load(args.plan)
    out = fresh(args.out.resolve())
    result = execute(plan, root, out, preflight_order(plan))
    load(args.plan)
    save(out / "result.json", result)


def measure(args):
    plan, root = load(args.plan)
    if not args.quiet_window:
        raise SystemExit("--quiet-window is required")
    checked_preflight(plan, root, args.preflight)
    out = fresh(args.out.resolve())
    result = execute(plan, root, out, plan["order"])
    load(args.plan)
    result["preflight_sha256"] = sha(args.preflight)
    save(out / "complete.json", result)


def summarize(args):
    plan, root = load(args.plan, check_environment=False)
    rows = [json.loads(line) for line in args.records.read_text().splitlines()]
    complete, failures = validate_rows(plan, root, args.records, rows, plan["order"])
    try:
        checked_preflight(plan, root, args.preflight)
        preflight_ok = True
    except SystemExit:
        preflight_ok = False
    try:
        receipt = json.loads((args.records.parent / "complete.json").read_text())
        receipt_ok = (receipt["pass"] is True and not receipt["failures"] and receipt["plan_sha256"] == sha(args.plan)
                      and receipt["preflight_sha256"] == sha(args.preflight)
                      and receipt["rows"] == len(rows) and receipt["records_sha256"] == sha(args.records))
    except (OSError, KeyError, ValueError, TypeError):
        receipt_ok = False
    result = {"schema": SCHEMA, "plan_sha256": sha(args.plan), "records_sha256": sha(args.records),
              "source_identity": plan["source_identity"], "selected_cases": plan["selected_cases"],
              "selected_original_weight": plan["selected_original_weight"], "phases": plan["phases"],
              "rows": len(rows), "coverage_complete": complete, "failures": failures,
              "completion_receipt_matches": receipt_ok, "preflight_matches": preflight_ok,
              "native_checks": sum(len(r.get("checks", [])) for r in rows), "scope": plan["scope"],
              "product_gates": "NotRun", "independent_review": "NotRun", "by_phase": {}}
    if failures or not complete or not receipt_ok or not preflight_ok:
        result["verdict"] = "IncompleteOrFailed_NoScore"
        save(args.out, result)
        raise SystemExit("Missing/failed cells retained; no aggregate score")
    blocks = list(range(plan["blocks"]))
    for phase in plan["phases"]:
        selected = [r for r in rows if r["phase"] == phase]
        values = {"wall": legacy.aggregate(plan, selected, blocks, "wall_seconds"),
                  "cpu": legacy.aggregate(plan, selected, blocks, "cpu_seconds")}
        rng = random.Random(plan["seed"])
        samples = sorted(legacy.aggregate(plan, selected, rng.choices(blocks, k=len(blocks)), "wall_seconds")["B_over_A"]
                         for _ in range(2000))
        values["exploratory_paired_block_bootstrap95"] = [samples[49], samples[1949]]
        values["by_case_request"] = {}
        for case in plan["cases"]:
            for request in REQUESTS:
                costs = {arm: statistics.median(statistics.mean(r["wall_seconds"] for r in selected
                                 if r["block"] == block and r["case"] == case["id"] and r["request"] == request and r["arm"] == arm)
                                 for block in blocks) for arm in "AB"}
                values["by_case_request"][case["id"] + "/" + request] = {**costs, "B_over_A": costs["B"] / costs["A"]}
        if phase == "build":
            digests = {}
            for r in selected:
                digests.setdefault((r["case"], r["snapshot"]), set()).add(r["executable_sha256"])
            values["same_binary_for_each_case_snapshot"] = all(len(s) == 1 for s in digests.values())
        result["by_phase"][phase] = values
    result["verdict"] = "ExploratoryComplete_NotAProductGate"
    result["bootstrap_rule"] = plan["bootstrap_rule"]
    save(args.out, result)
    print(json.dumps(result, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare")
    for name in ("out", "baseline", "candidate", "clang"):
        p.add_argument("--" + name, type=Path, required=True)
    for name in ("baseline-source", "candidate-source"):
        p.add_argument("--" + name, required=True)
    p.add_argument("--tool", type=Path, action="append", default=[], help="Freeze an actual LLVM shared library/linker file; repeatable")
    p.add_argument("--case", action="append", help="Freeze a subset; original and normalized weights are recorded")
    p.add_argument("--phase", action="append", choices=["check", "build"])
    p.add_argument("--blocks", type=int, choices=[3, 5], default=3)
    p.add_argument("--profile", choices=["O0", "O2"], default="O0")
    p.add_argument("--units", type=int, choices=range(1, 65), default=1)
    p.add_argument("--jobs", type=int, choices=range(1, 65), default=1)
    p.add_argument("--object-cache", action="store_true", help="Enable identically on both arms")
    p.add_argument("--cpus"); p.add_argument("--seed", type=int, default=20261029)
    p.add_argument("--timeout", type=int, default=180)
    p.set_defaults(run=prepare)
    for action, handler in (("preflight", preflight), ("measure", measure), ("summarize", summarize)):
        p = sub.add_parser(action)
        p.add_argument("--plan", type=Path, required=True); p.add_argument("--out", type=Path, required=True)
        if action in ("measure", "summarize"):
            p.add_argument("--preflight", type=Path, required=True)
        if action == "measure":
            p.add_argument("--quiet-window", action="store_true")
        if action == "summarize":
            p.add_argument("--records", type=Path, required=True)
        p.set_defaults(run=handler)
    args = parser.parse_args()
    if getattr(args, "timeout", 1) <= 0:
        parser.error("--timeout must be positive")
    args.run(args)


if __name__ == "__main__":
    main()

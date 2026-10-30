#!/usr/bin/env python3
"""CP04: frozen native runtime quality, separate from compiler timing.

Prepare inputs before building; build and correctness/startup preflight are not
runtime samples. Measure only the resulting fixed executables, with no tracing.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import importlib.util
import json
import math
import os
from pathlib import Path
import random
import shutil
import signal
import statistics
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("cperf_common", HERE / "bench.py")
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
ROOT, sha, save, fresh = common.ROOT, common.sha, common.save, common.fresh
SCHEMA = "zil-cperf-native-runtime/1"
INPUT_BYTES = {"tree4x2": 1048576, "tree32x8": 32768, "tree256x8": 4096,
               "tree1024x8": 1024, "retained": 262144}
HELPERS = ["tools/research/compile-performance/bench.py", *common.DEPENDENCIES]


def fixture_hashes(root):
    return {prefix + "/" + name: digest for prefix in ("sources", "inputs", "expected")
            for name, digest in common.file_hashes(root / prefix).items()}


def prepare(args):
    if sys.platform != "linux" or not hasattr(os, "wait4"):
        raise SystemExit("Linux affinity and wait4 required")
    if common.git("status", "--porcelain"):
        raise SystemExit("Commit this runner before freezing a plan")
    allowed = sorted(os.sched_getaffinity(0))
    cpu = allowed[0] if args.cpu is None else args.cpu
    if cpu not in allowed:
        raise SystemExit("--cpu is outside the allowed affinity")
    upstream_path = args.corpus_plan.resolve()
    upstream = json.loads(upstream_path.read_text())
    if upstream.get("schema") != "zil-compile-performance/1":
        raise SystemExit("Expected the frozen CP01/CP03 corpus plan")
    if {c["id"] for c in upstream["cases"]} != set(INPUT_BYTES) or len(upstream["cases"]) != 5:
        raise SystemExit("Runtime contract requires all five frozen cases")
    for helper in ("tools/research/210/corpus.py", "tools/product/109/reference104/run.py",
                   "tools/product/109/reference104/contract.json"):
        if sha(ROOT / helper) != upstream["dependencies_sha256"][helper]:
            raise SystemExit("Inherited oracle changed: " + helper)
    binaries = {"A": common.inventory(args.baseline), "B": common.inventory(args.candidate),
                "clang": common.inventory(args.clang)}
    identities = {"A": common.source_identity(args.baseline_source), "B": common.source_identity(args.candidate_source)}
    out = fresh(args.out.resolve())
    corpus = common.module("cperf_runtime_corpus", ROOT / "tools/research/210/corpus.py")
    oracle = corpus.reference()
    (out / "inputs").mkdir(); (out / "expected").mkdir()
    (out / "inputs/empty.bin").write_bytes(b"")
    (out / "expected/empty.bin").write_bytes(b"")
    cases = []
    for original in upstream["cases"]:
        name = original["id"]
        source = upstream_path.parent / original["root"] / "base/zil"
        prefix = original["root"].removeprefix("corpus/") + "/base/zil/"
        expected_sources = {k[len(prefix):]: v for k, v in upstream["fixtures_sha256"].items() if k.startswith(prefix)}
        if not expected_sources or common.file_hashes(source) != expected_sources:
            raise SystemExit("Original base sources changed: " + name)
        shutil.copytree(source, out / "sources" / name)
        size = INPUT_BYTES[name]
        if name == "retained":
            data = oracle.input_bytes("retained", size)
            expected = oracle.model("retained", data)[0]
        else:
            data = bytes((i * 73 + 19) % 256 for i in range(size))
            expected = corpus.tree_expected(original["modules_requested"], original["width"], "base", data)
        input_path, expected_path = f"inputs/{name}.bin", f"expected/{name}.bin"
        (out / input_path).write_bytes(data); (out / expected_path).write_bytes(expected)
        cases.append({"id": name, "weight": original["weight"], "source": f"sources/{name}",
                      "input": input_path, "expected": expected_path, "input_bytes": size,
                      "source_sha256": expected_sources, "kind": original["kind"],
                      "generated_scalar_calls_per_run": None if name == "retained"
                          else size * original["modules_requested"] * original["width"]})
    if not math.isclose(sum(c["weight"] for c in cases), 1):
        raise SystemExit("Original five-case weights must sum to one")
    rng, order = random.Random(args.seed), []
    for block in range(args.blocks):
        names = [c["id"] for c in cases]
        rng.shuffle(names)
        for name in names:
            order.extend({"block": block, "case": name, "position": position, "arm": arm}
                         for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"))
    plan = {"schema": SCHEMA, "created_unix": time.time(), "source_identity": identities, "runner_source_commit": common.git("rev-parse", "HEAD"),
            "source_binding": "caller-declared compiler commits; independent compiler/executable hashes and build logs",
            "runner_sha256": sha(Path(__file__)), "helper_sha256": {p: sha(ROOT / p) for p in HELPERS},
            "upstream_corpus_plan": {"path": str(upstream_path), "sha256": sha(upstream_path)},
            "binaries": binaries, "extra_tools": [common.inventory(p) for p in args.tool],
            "versions": {name: common.capture([v["path"], "--version"]) for name, v in binaries.items()},
            "environment": common.environment(), "host": common.host(), "cpu": cpu,
            "profile": args.profile, "object_cache": False, "timings": False,
            "build_config": {arm: {"profile": args.profile,
                                   "units": getattr(args, label + "_units"),
                                   "jobs": getattr(args, label + "_jobs"),
                                   "closed_executable": getattr(args, label + "_closed_executable")}
                             for arm, label in (("A", "baseline"), ("B", "candidate"))},
            "cases": cases, "blocks": args.blocks, "seed": args.seed, "order": order,
            "expected_rows": len(order), "fixtures_sha256": fixture_hashes(out),
            "build_timeout_seconds": 180, "runtime_timeout_seconds": args.timeout,
            "scope": "Full native process wall/CPU including startup and stdio; file identities checked before runs, OS caches not flushed; no compiler work or profiling in runtime samples",
            "input_rationale": "Four synthetic cases each execute 8388608 generated scalar function calls, plus module/IO/effect calls; retained uses fixed 262144-byte consumer input. Counts are source estimates, not timing claims.",
            "preflight": "One empty-input startup diagnostic and one full-input correctness/warmup run per executable; neither enters the measured sample or is subtracted",
            "retention": "Keep frozen sources/inputs/oracle, native executables, raw stdout/stderr and receipts. Generated .ll files are removed only after hashing.",
            "failure_rule": "Every frozen cell required; any failure/timeout/hash drift suppresses all scores",
            "product_gates": "NotRun"}
    save(out / "plan.json", plan)
    print(out / "plan.json")


def load(plan_path, environment=True):
    plan_path = plan_path.resolve()
    plan = json.loads(plan_path.read_text())
    if plan["schema"] != SCHEMA or sha(Path(__file__)) != plan["runner_sha256"]:
        raise SystemExit("Frozen runtime runner changed")
    for path, digest in plan["helper_sha256"].items():
        if sha(ROOT / path) != digest:
            raise SystemExit("Frozen helper/oracle changed: " + path)
    if fixture_hashes(plan_path.parent) != plan["fixtures_sha256"]:
        raise SystemExit("Frozen source/input/oracle set changed")
    for binary in [*plan["binaries"].values(), *plan["extra_tools"]]:
        if common.inventory(binary["path"]) != binary:
            raise SystemExit("Frozen compiler/library changed: " + binary["path"])
    if environment:
        if common.environment() != plan["environment"] or plan["cpu"] not in os.sched_getaffinity(0):
            raise SystemExit("Frozen environment/affinity changed")
        current = common.host()
        if any(current[k] != plan["host"][k] for k in ("cpu_model", "cpu_max", "memory_max")):
            raise SystemExit("Frozen host/quota changed")
    return plan, plan_path.parent


def timed_native(executable, source, stdout, stderr, cpu, timeout):
    expired = threading.Event()
    with source.open("rb") as data, stdout.open("wb") as output, stderr.open("wb") as errors:
        start = time.perf_counter_ns()
        child = subprocess.Popen([str(executable)], stdin=data, stdout=output, stderr=errors,
                                 preexec_fn=lambda: os.sched_setaffinity(0, [cpu]), start_new_session=True)
        def kill():
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
    return {"wall_seconds": (stop-start)/1e9, "cpu_user": usage.ru_utime, "cpu_system": usage.ru_stime,
            "exit": child.returncode, "timeout": expired.is_set(), "max_process_rss_kib": usage.ru_maxrss}


def run_native(plan, root, executable, input_path, expected_path, logs, tag):
    stdout, stderr = logs / (tag + ".stdout.bin"), logs / (tag + ".stderr.bin")
    row = {"executable": executable, "input": input_path, "expected": expected_path,
           "stdout": stdout.name, "stderr": stderr.name, "pass": False}
    try:
        source = root / input_path
        row["input_sha256"] = sha(source)
        row["expected_stdout_sha256"] = plan["fixtures_sha256"][expected_path]
        identity_ok = common.inventory(executable["path"]) == executable
        if not identity_ok or row["input_sha256"] != plan["fixtures_sha256"][input_path]:
            row["identity_drift"] = True
            return row
        row.update(timed_native(Path(executable["path"]), source, stdout, stderr, plan["cpu"], plan["runtime_timeout_seconds"]))
        row.update(stdout_sha256=sha(stdout), stderr_sha256=sha(stderr), stdout_bytes=stdout.stat().st_size,
                   input_after_sha256=sha(source), executable_after=common.inventory(executable["path"]))
        row["pass"] = (row["exit"] == 0 and not row["timeout"] and row["stdout_sha256"] == row["expected_stdout_sha256"]
                       and stderr.stat().st_size == 0 and row["input_after_sha256"] == row["input_sha256"]
                       and row["executable_after"] == executable)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        row["error"] = repr(error)
    return row


def build_config(plan, arm):
    """Old CP04 plans retain their original O0/one-unit/default-linkage contract."""
    return plan.get("build_config", {}).get(arm, {
        "profile": "O0", "units": 1, "jobs": 1, "closed_executable": False})


def build_argv(plan, root, case, arm, executable):
    config = build_config(plan, arm)
    argv = [plan["binaries"][arm]["path"], "build", str(root / case["source"] / "main.zil"),
            "-o", str(executable), "--json", "--backend", "llvm-api", "--opt", config["profile"],
            "--codegen-units", str(config["units"]), "--codegen-jobs", str(config["jobs"]),
            "--clang", plan["binaries"]["clang"]["path"]]
    if config["closed_executable"]:
        argv.append("--closed-executable")
    return argv


def reported_mode_matches(plan, arm, report):
    native = report.get("native_codegen") or {}
    observed = native.get("closed_executable", False) if isinstance(native, dict) else None
    return isinstance(observed, bool) and observed == build_config(plan, arm)["closed_executable"]


def build(args):
    plan, root = load(args.plan)
    out, rows = fresh(args.out.resolve()), []
    logs = fresh(out / "logs")
    plan_hash = sha(args.plan)
    with (out / "builds.jsonl").open("x") as stream:
        for case in plan["cases"]:
            for arm in "AB":
                tag = case["id"] + "-" + arm
                folder = fresh(out / "executables" / tag)
                executable = folder / "program"
                argv = build_argv(plan, root, case, arm, executable)
                row = {"case": case["id"], "arm": arm, "plan_sha256": plan_hash, "argv": argv,
                       "stdout": tag + ".build.stdout.json", "stderr": tag + ".build.stderr.txt", "pass": False}
                try:
                    row["source_before"] = common.file_hashes(root / case["source"])
                    row["build_process"] = common.legacy.timed(argv, logs / row["stdout"], logs / row["stderr"], [plan["cpu"]], plan["build_timeout_seconds"])
                    row.update(stdout_sha256=sha(logs / row["stdout"]), stderr_sha256=sha(logs / row["stderr"]))
                    report = json.loads((logs / row["stdout"]).read_text())
                    row["source_after"] = common.file_hashes(root / case["source"])
                    row["compiler_status"] = report.get("status")
                    row["verification_runs"] = report.get("verification_runs")
                    row["timings_disabled"] = report.get("timings") is None
                    row["pass"] = (row["build_process"]["exit"] == 0 and not row["build_process"]["timeout"]
                                   and row["compiler_status"] == "Success" and row["verification_runs"] == 1
                                   and row["timings_disabled"] and reported_mode_matches(plan, arm, report)
                                   and row["source_before"] == row["source_after"] == case["source_sha256"])
                    if row["pass"]:
                        row["executable"] = common.inventory(executable)
                        row["llvm_sha256"] = sha(executable.with_suffix(".ll"))
                        executable.with_suffix(".ll").unlink()
                        row["startup"] = run_native(plan, root, row["executable"], "inputs/empty.bin", "expected/empty.bin", logs, tag + ".startup")
                        row["warmup"] = run_native(plan, root, row["executable"], case["input"], case["expected"], logs, tag + ".warmup")
                        row["pass"] &= row["startup"]["pass"] and row["warmup"]["pass"]
                except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
                    row.update(error=repr(error), **{"pass": False})
                rows.append(row)
                stream.write(json.dumps(row) + "\n"); stream.flush()
                print("build/preflight", tag, row["pass"], flush=True)
    save(out / "result.json", {"plan_sha256": plan_hash, "rows": len(rows), "pass": all(r["pass"] for r in rows),
                              "failures": [r["case"] + "/" + r["arm"] for r in rows if not r["pass"]],
                              "builds_sha256": sha(out / "builds.jsonl"), "runtime_samples": 0})


def built(plan, plan_path, receipt_path):
    receipt = json.loads(receipt_path.read_text())
    path = receipt_path.parent / "builds.jsonl"
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    expected = [(c["id"], arm) for c in plan["cases"] for arm in "AB"]
    if (not receipt.get("pass") or receipt["plan_sha256"] != sha(plan_path) or receipt["builds_sha256"] != sha(path)
            or receipt["rows"] != len(expected) or [(r["case"], r["arm"]) for r in rows] != expected
            or any(not r["pass"] or r["plan_sha256"] != sha(plan_path) for r in rows)):
        raise SystemExit("Complete matching successful build/preflight required")
    for row in rows:
        case = next(c for c in plan["cases"] if c["id"] == row["case"])
        if row["argv"] != build_argv(plan, plan_path.resolve().parent, case, row["arm"], row["executable"]["path"]):
            raise SystemExit("Frozen build configuration changed: " + row["case"] + "/" + row["arm"])
        if common.inventory(row["executable"]["path"]) != row["executable"]:
            raise SystemExit("Native executable changed after build: " + row["case"] + "/" + row["arm"])
        for stream in ("stdout", "stderr"):
            if sha(receipt_path.parent / "logs" / row[stream]) != row[stream + "_sha256"]:
                raise SystemExit("Raw build output changed: " + row[stream])
        report = json.loads((receipt_path.parent / "logs" / row["stdout"]).read_text())
        if not reported_mode_matches(plan, row["arm"], report):
            raise SystemExit("Compiler did not confirm frozen closed-executable mode")
        for stage in ("startup", "warmup"):
            check = row[stage]
            if (not check["pass"] or check["exit"] != 0 or check["timeout"]
                    or not check["executable"] == check["executable_after"] == row["executable"]
                    or check["stdout_sha256"] != plan["fixtures_sha256"][check["expected"]]):
                raise SystemExit("Successful native preflight required: " + stage)
            for stream in ("stdout", "stderr"):
                log = receipt_path.parent / "logs" / check[stream]
                if sha(log) != check[stream + "_sha256"] or (stream == "stderr" and log.stat().st_size):
                    raise SystemExit("Raw native preflight output changed: " + check[stream])
    return {(r["case"], r["arm"]): r for r in rows}


def measure(args):
    plan, root = load(args.plan)
    if not args.quiet_window:
        raise SystemExit("Explicit --quiet-window required; do not compile concurrently")
    artifacts = built(plan, args.plan, args.build_receipt)
    out, failures = fresh(args.out.resolve()), []
    logs = fresh(out / "logs")
    cases = {c["id"]: c for c in plan["cases"]}
    plan_hash, receipt_hash = sha(args.plan), sha(args.build_receipt)
    with (out / "records.jsonl").open("x") as stream:
        for index, seq in enumerate(plan["order"]):
            case = cases[seq["case"]]
            row = run_native(plan, root, artifacts[(seq["case"], seq["arm"])]["executable"], case["input"], case["expected"], logs, f"{index:04d}")
            row.update(seq, index=index, plan_sha256=plan_hash, build_receipt_sha256=receipt_hash)
            stream.write(json.dumps(row) + "\n"); stream.flush()
            if not row["pass"]:
                failures.append(index)
            print(index+1, len(plan["order"]), seq["case"], seq["arm"], row["pass"], flush=True)
    save(out / "complete.json", {"plan_sha256": plan_hash, "build_receipt_sha256": receipt_hash,
                                "rows": len(plan["order"]), "failures": failures, "records_sha256": sha(out / "records.jsonl")})


def aggregate(plan, rows, blocks, metric):
    cells = defaultdict(list)
    for row in rows:
        value = row["cpu_user"] + row["cpu_system"] if metric == "cpu_seconds" else row[metric]
        cells[(row["block"], row["case"], row["arm"])].append(value)
    totals, cases = {"A": 0., "B": 0.}, {}
    for case in plan["cases"]:
        values = {arm: statistics.median(statistics.mean(cells[(block, case["id"], arm)]) for block in blocks) for arm in "AB"}
        for arm in "AB":
            totals[arm] += case["weight"] * values[arm]
        values["B_over_A"] = values["B"] / values["A"]
        cases[case["id"]] = values
    return {**totals, "B_over_A": totals["B"] / totals["A"], "by_case": cases}


def summarize(args):
    plan, _ = load(args.plan, environment=False)
    artifacts = built(plan, args.plan, args.build_receipt)
    rows = [json.loads(line) for line in args.records.read_text().splitlines()]
    key = lambda row: tuple(row.get(k) for k in ("block", "case", "position", "arm"))
    complete = [key(r) for r in rows] == [key(r) for r in plan["order"]]
    plan_hash, build_hash = sha(args.plan), sha(args.build_receipt)
    try:
        receipt = json.loads((args.records.parent / "complete.json").read_text())
        receipt_ok = (receipt["plan_sha256"] == plan_hash and receipt["build_receipt_sha256"] == build_hash
                      and receipt["records_sha256"] == sha(args.records) and receipt["rows"] == len(rows)
                      and not receipt["failures"])
    except (OSError, ValueError, KeyError):
        receipt_ok = False
    failures = []
    cases = {c["id"]: c for c in plan["cases"]}
    for index, row in enumerate(rows):
        try:
            case = cases[row["case"]]
            ok = (row["pass"] and row["exit"] == 0 and not row["timeout"] and row["index"] == index
                  and row["plan_sha256"] == plan_hash and row["build_receipt_sha256"] == build_hash
                  and row["executable"] == row["executable_after"] == artifacts[(row["case"], row["arm"])]["executable"]
                  and row["input"] == case["input"] and row["expected"] == case["expected"]
                  and row["input_sha256"] == row["input_after_sha256"] == plan["fixtures_sha256"][case["input"]]
                  and row["stdout_sha256"] == row["expected_stdout_sha256"] == plan["fixtures_sha256"][case["expected"]]
                  and all(isinstance(row[k], (int, float)) and math.isfinite(row[k]) and row[k] >= 0 for k in ("wall_seconds", "cpu_user", "cpu_system"))
                  and row["wall_seconds"] > 0 and row["cpu_user"] + row["cpu_system"] > 0)
            for stream in ("stdout", "stderr"):
                path = args.records.parent / "logs" / row[stream]
                ok &= sha(path) == row[stream + "_sha256"]
                if stream == "stderr":
                    ok &= path.stat().st_size == 0
        except (OSError, KeyError, ValueError):
            ok = False
        if not ok:
            failures.append(index)
    result = {"schema": SCHEMA, "plan_sha256": plan_hash, "build_receipt_sha256": build_hash,
              "records_sha256": sha(args.records), "source_identity": plan["source_identity"],
              "build_config": {arm: build_config(plan, arm) for arm in "AB"},
              "rows": len(rows), "coverage_complete": complete, "completion_receipt_matches": receipt_ok, "failures": failures,
              "scope": plan["scope"], "product_gates": "NotRun", "independent_review": "NotRun"}
    if not complete or not receipt_ok or failures:
        result["verdict"] = "IncompleteOrFailed_NoScore"
        save(args.out, result)
        raise SystemExit("All runtime cells and native checks are required; no score")
    blocks = list(range(plan["blocks"]))
    result["wall"] = aggregate(plan, rows, blocks, "wall_seconds")
    result["cpu"] = aggregate(plan, rows, blocks, "cpu_seconds")
    rng = random.Random(plan["seed"])
    samples = sorted(aggregate(plan, rows, rng.choices(blocks, k=len(blocks)), "wall_seconds")["B_over_A"] for _ in range(5000))
    result["exploratory_paired_block_bootstrap95"] = [samples[124], samples[4874]]
    result["startup_diagnostics"] = {case["id"]: {arm: {
        "single_preflight_empty_seconds": artifacts[(case["id"], arm)]["startup"]["wall_seconds"],
        "empty_over_measured_full_seconds": artifacts[(case["id"], arm)]["startup"]["wall_seconds"] / result["wall"]["by_case"][case["id"]][arm],
        "executable_bytes": artifacts[(case["id"], arm)]["executable"]["size"],
        "executable_sha256": artifacts[(case["id"], arm)]["executable"]["sha256"]} for arm in "AB"} for case in plan["cases"]}
    result["startup_note"] = "One empty-input preflight observation per executable; diagnostic only, not subtracted or used to remove samples"
    result["verdict"] = "ExploratoryRuntimeQuality_NotAProductGate"
    save(args.out, result)
    print(json.dumps(result, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare")
    for name in ("out", "corpus-plan", "baseline", "candidate", "clang"):
        p.add_argument("--" + name, type=Path, required=True)
    for name in ("baseline-source", "candidate-source"):
        p.add_argument("--" + name, required=True)
    p.add_argument("--tool", type=Path, action="append", default=[])
    p.add_argument("--profile", choices=["O0", "O2"], default="O0")
    for label in ("baseline", "candidate"):
        p.add_argument("--" + label + "-units", type=int, choices=range(1, 65), default=1)
        p.add_argument("--" + label + "-jobs", type=int, choices=range(1, 65), default=1)
        p.add_argument("--" + label + "-closed-executable", action="store_true")
    p.add_argument("--cpu", type=int); p.add_argument("--blocks", type=int, choices=[3, 5], default=5)
    p.add_argument("--seed", type=int, default=20261004); p.add_argument("--timeout", type=int, default=30)
    p.set_defaults(run=prepare)
    for action, handler in (("build", build), ("measure", measure), ("summarize", summarize)):
        p = sub.add_parser(action)
        p.add_argument("--plan", type=Path, required=True); p.add_argument("--out", type=Path, required=True)
        if action in ("measure", "summarize"):
            p.add_argument("--build-receipt", type=Path, required=True)
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

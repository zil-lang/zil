#!/usr/bin/env python3
"""CP08: frozen full-build and native-runtime comparison, Rust A versus ZIL B.

Linux/Python standard library only. Reuses 210 paired sources/oracles, 212
wait4/aggregation and CP04 native execution; never changes historical runners.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import random
import shutil
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("cp08_common", HERE / "bench.py")
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
runtime = common.module("cp08_runtime", HERE / "runtime_bench.py")
ROOT, sha, save, fresh = common.ROOT, common.sha, common.save, common.fresh
SCHEMA = "zil-rust-compile-performance/3"
TARGET = "x86_64-unknown-linux-gnu"
HELPERS = ["tools/research/compile-performance/bench.py",
           "tools/research/compile-performance/runtime_bench.py", *common.DEPENDENCIES]


def fixtures(root):
    return {prefix + "/" + name: digest for prefix in ("corpus", "inputs", "expected")
            for name, digest in common.file_hashes(root / prefix).items()}


def order_for(cases, blocks, seed):
    rng, result = random.Random(seed), []
    for block in range(blocks):
        names = [case["id"] for case in cases]
        rng.shuffle(names)
        for name in names:
            result.extend({"block": block, "case": name, "position": pos, "arm": arm}
                          for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"))
    return result


def tool_identity(path):
    """Keep argv[0]/library aliases as well as the resolved file identity."""
    path = Path(path).absolute()
    identity = common.inventory(path)
    return {**identity, "path": str(path), "resolved_path": identity["path"],
            "symlink_target": os.readlink(path) if path.is_symlink() else None}


def linker_candidates(clang, rust_linker, search_path, zil_override):
    # Re-evaluate this search at load, including absent candidates: adding a
    # higher-priority PATH entry or a new Clang sibling also changes selection.
    candidates = {}
    for name in ("cc", "gcc", "ld", "ld.lld", "as"):
        found = shutil.which(name, path=search_path if search_path is not None else os.defpath)
        candidates[name] = str(Path(found).absolute()) if found else None
    sibling = Path(clang).with_name("ld.lld")
    candidates["clang_sibling_lld"] = str(sibling.absolute()) if sibling.is_file() else None
    candidates["rust_explicit"] = rust_linker
    candidates["zil_explicit"] = None
    if zil_override is not None and zil_override != "system":
        found = shutil.which(zil_override, path=search_path if search_path is not None else os.defpath)
        if not found:
            raise SystemExit("ZIL_LINUX_LINKER must be 'system' or an executable path/name")
        candidates["zil_explicit"] = str(Path(found).absolute())
    return candidates


def tool_inventory(rustc, clang, extra, rust_linker):
    """Freeze rust sysroot artifacts and possible native linkers, not only rustc."""
    probe = common.capture([rustc, "--print", "sysroot"])
    if probe.get("exit") != 0:
        raise SystemExit("Cannot identify the Rust sysroot: " + str(probe))
    sysroot = Path(probe["stdout"].strip()).absolute()
    paths = {Path(p).absolute() for p in extra}
    target = sysroot / "lib/rustlib" / TARGET
    # std/alloc/core, bundled LLVM, rustc_driver and bundled linker are inputs
    # even though their compilation is not repeated inside this benchmark.
    paths.update(p for p in (sysroot / "lib").glob("*.so*") if p.is_file())
    paths.update(p for p in (target / "lib").rglob("*") if p.is_file())
    paths.update(p for p in (target / "bin").rglob("*") if p.is_file())
    search_path, zil_override = os.environ.get("PATH"), os.environ.get("ZIL_LINUX_LINKER")
    candidates = linker_candidates(clang, rust_linker, search_path, zil_override)
    paths.update(Path(p) for p in candidates.values() if p is not None)
    return [tool_identity(p) for p in sorted(paths)], {
        "sysroot_probe": probe, "sysroot": str(sysroot), "linker_candidates": candidates,
        "linker_search_path": search_path, "zil_linker_override": zil_override,
        "rust_linker": rust_linker or "toolchain-native default",
        "zil_linker": "clang driver; actual argv retained in every ZIL JSON report",
        "boundary": "sysroot artifacts and listed tools frozen; system libc/crt and OS loader are shared host dependencies, not rebuilt; native Rust linker selection is not inferred from executable size"}


def validate_modes(cache_mode, query_cache, closed_executable, rust_lto):
    if type(query_cache) is not bool or type(closed_executable) is not bool:
        raise SystemExit("Explicit boolean query-cache and closed-executable modes required")
    if query_cache and cache_mode != "incremental":
        raise SystemExit("--query-cache requires --cache-mode incremental; cold means no compiler caches")
    if rust_lto not in ("native", "off"):
        raise SystemExit("Explicit --rust-lto native or off required in the frozen plan")


def prepare(args):
    validate_modes(args.cache_mode, args.query_cache, args.closed_executable, args.rust_lto)
    if sys.platform != "linux" or not hasattr(os, "wait4"):
        raise SystemExit("Linux affinity and wait4 required")
    if common.git("status", "--porcelain"):
        raise SystemExit("Commit runner/source before prepare; never overwrite an old plan")
    allowed = sorted(os.sched_getaffinity(0))
    cpus = sorted(set(map(int, args.cpus.split(",")))) if args.cpus else allowed[:min(4, len(allowed))]
    if not cpus or not set(cpus) <= set(allowed):
        raise SystemExit("--cpus must be a nonempty subset of the allowed affinity")
    binaries = {"A": tool_identity(args.rustc), "B": tool_identity(args.zil),
                "clang": tool_identity(args.clang)}
    rust_linker = str(args.rust_linker.absolute()) if args.rust_linker else None
    extra, linkage = tool_inventory(binaries["A"]["path"], binaries["clang"]["path"], args.tool, rust_linker)
    versions = {"rustc": common.capture([binaries["A"]["path"], "-Vv"]),
                "zil": common.capture([binaries["B"]["path"], "--version"]),
                "clang": common.capture([binaries["clang"]["path"], "--version"])}
    if versions["rustc"].get("exit") != 0 or "host: " + TARGET not in versions["rustc"].get("stdout", ""):
        raise SystemExit("This corpus contract requires a native x86_64-unknown-linux-gnu Rust compiler")
    out = fresh(args.out.resolve())
    corpus = common.module("cp08_corpus", ROOT / "tools/research/210/corpus.py")
    cases = corpus.generate(out)
    selected = set(args.case or [c["id"] for c in cases])
    if selected - {c["id"] for c in cases}:
        raise SystemExit("Unknown case; generated files retained at " + str(out))
    cases = [c for c in cases if c["id"] in selected]
    original_weight = sum(c["weight"] for c in cases)
    (out / "inputs").mkdir(); (out / "expected").mkdir()
    (out / "inputs/empty.bin").write_bytes(b""); (out / "expected/empty.bin").write_bytes(b"")
    oracle = corpus.reference()
    for case in cases:
        name = case["id"]
        case["original_weight"] = case["weight"]
        case["weight"] /= original_weight
        size = runtime.INPUT_BYTES[name]
        data = oracle.input_bytes("retained", size) if name == "retained" else bytes((i*73+19) % 256 for i in range(size))
        expected = oracle.model("retained", data)[0] if name == "retained" else corpus.tree_expected(case["modules_requested"], case["width"], "base", data)
        case.update(input=f"inputs/{name}.bin", expected=f"expected/{name}.bin", input_bytes=size)
        (out / case["input"]).write_bytes(data); (out / case["expected"]).write_bytes(expected)
    dependencies = HELPERS + [str(p.relative_to(ROOT)) for p in sorted(corpus.CONSUMER.rglob("*")) if p.is_file()]
    plan = {
        "schema": SCHEMA, "created_unix": time.time(), "runner_source_commit": common.git("rev-parse", "HEAD"),
        "source_identity": {"B": common.source_identity(args.zil_source)},
        "source_binding": "ZIL source commit declared by caller; binary hash frozen separately; keep actual build receipt; Rust provenance is -Vv plus frozen toolchain",
        "runner_sha256": sha(Path(__file__)), "helper_sha256": {p: sha(ROOT / p) for p in dependencies},
        "binaries": binaries, "extra_tools": extra, "linkage": linkage, "versions": versions,
        "environment": common.environment(), "host": common.host(), "cpus": cpus, "cpu": cpus[0],
        "target": TARGET, "profile": args.profile, "cache_mode": args.cache_mode,
        "query_cache": args.query_cache, "closed_executable": args.closed_executable,
        "rust_profile": args.rust_profile, "rust_linker": rust_linker, "rust_lto": args.rust_lto,
        "units": args.units, "jobs": args.jobs, "blocks": args.blocks, "seed": args.seed,
        "request_weights": common.REQUESTS if args.cache_mode == "incremental" else {"cold_process": 1.0},
        "cases": cases, "selected_original_weight": original_weight, "order": order_for(cases, args.blocks, args.seed),
        "fixtures_sha256": fixtures(out), "timeout_seconds": args.timeout, "runtime_timeout_seconds": args.runtime_timeout,
        "scope": "Full compiler process to complete executable, with current Verify/lowering/codegen/link/publication; no Cargo no-op, no persistent-session shortcut; OS pages not flushed",
        "cache_scope": "cold: no ZIL source/object/query cache or Rust incremental; incremental: fresh sequence then unchanged/private/public using isolated per-arm caches; explicit query_cache requires a new current Verify and only identical whole-input hits",
        "linkage_scope": "closed_executable explicitly permits internalizing generated ZIL-only symbols in complete Linux executables; compare runtime quality independently; Rust native CGU and existing flags unchanged",
        "rust_lto_scope": "native omits -C lto and retains the toolchain policy; off explicitly disables Rust LTO for a separate mechanism diagnostic, never a replacement for native results; compare runtime quality for each mode",
        "runtime_scope": "Separate complete native processes including startup and file I/O on one CPU; base binaries from preflight only; compilation/correctness/warmup are not runtime samples",
        "warmup": "One complete AB preflight per case/request, plus empty-input startup and long-input warmup per base executable; no observations subtracted or scored",
        "cpu_accounting": "wait4 user+system includes reaped descendants; RSS is maximum individual process, not simultaneous process-tree peak",
        "comparison_limits": "80% synthetic called trees at default weights; retained uses Rust Vec and ZIL ByteBuffer; precompiled Rust std vs host libc; checked valid inputs do not establish identical panic/allocation-failure behavior or whole-language equivalence",
        "failure_rule": "Any failed/missing/timed-out row, changed frozen input/tool, or changed raw log prevents scoring; never retry into the same directory",
        "product_gates": "NotRun"}
    save(out / "plan.json", plan)
    print(out / "plan.json")


def load(path, check_environment=True):
    path = path.resolve()
    plan = json.loads(path.read_text())
    if plan.get("schema") != SCHEMA:
        raise SystemExit("Frozen schema changed; replay the old plan with its original runner/source, or prepare a new plan")
    if sha(Path(__file__)) != plan["runner_sha256"]:
        raise SystemExit("Frozen runner changed")
    validate_modes(plan["cache_mode"], plan["query_cache"], plan["closed_executable"], plan.get("rust_lto"))
    for name, digest in plan["helper_sha256"].items():
        if sha(ROOT / name) != digest:
            raise SystemExit("Frozen helper changed: " + name)
    if fixtures(path.parent) != plan["fixtures_sha256"]:
        raise SystemExit("Frozen source/input/oracle changed")
    for tool in [*plan["binaries"].values(), *plan["extra_tools"]]:
        if tool_identity(tool["path"]) != tool:
            raise SystemExit("Frozen toolchain changed: " + tool["path"])
    linkage = plan["linkage"]
    if linker_candidates(plan["binaries"]["clang"]["path"], plan["rust_linker"],
                         linkage["linker_search_path"], linkage["zil_linker_override"]) != linkage["linker_candidates"]:
        raise SystemExit("Frozen linker selection changed")
    if check_environment:
        now = common.host()
        if (common.environment() != plan["environment"] or not set(plan["cpus"]) <= os.sched_getaffinity(0)
                or any(now[k] != plan["host"][k] for k in ("cpu_model", "cpu_max", "memory_max"))):
            raise SystemExit("Frozen environment, affinity or host quota changed")
    return plan, path.parent


def command(plan, arm, work):
    output = work / "program"
    incremental = plan["cache_mode"] == "incremental"
    if arm == "A":
        argv = [plan["binaries"]["A"]["path"], str(work / "src/main.rs"), "--edition=2024", "--crate-name", "zil_cp08_ref",
                "-Awarnings", "--target", plan["target"], "-C", "target-cpu=x86-64", "-C", "opt-level=" + plan["profile"][1:],
                "-C", "debuginfo=0", "-C", "overflow-checks=on", "-C", "panic=abort", "-C", "embed-bitcode=no", "-o", str(output)]
        if incremental:
            argv += ["-C", "incremental=" + str(work / "rust-incremental")]
        if plan["rust_profile"] == "matched":
            argv += ["-C", "codegen-units=" + str(plan["units"])]
        if plan["rust_lto"] == "off":
            argv += ["-C", "lto=off"]
        if plan["rust_linker"]:
            argv += ["-C", "linker=" + plan["rust_linker"]]
        return argv
    argv = [plan["binaries"]["B"]["path"], "build", str(work / "src/main.zil"), "-o", str(output), "--json",
            "--backend", "llvm-api", "--clang", plan["binaries"]["clang"]["path"], "--opt", plan["profile"],
            "--codegen-units", str(plan["units"]), "--codegen-jobs", str(plan["jobs"])]
    if incremental:
        argv += ["--cache-dir", str(work / "source-candidates"), "--object-cache", str(work / "objects")]
    if plan["query_cache"]:
        argv += ["--query-cache", str(work / "queries")]
    if plan["closed_executable"]:
        argv += ["--closed-executable"]
    return argv


def source_hashes(plan, case, snapshot, arm):
    prefix = case["root"] + "/" + snapshot + ("/rust/" if arm == "A" else "/zil/")
    return {path[len(prefix):]: digest for path, digest in plan["fixtures_sha256"].items() if path.startswith(prefix)}


def report_ok(plan, request, report):
    if not isinstance(report, dict) or not isinstance(report.get("native_codegen", {}), dict):
        return False
    ok = (report.get("status") == "Success" and report.get("verification_runs") == 1 and report.get("timings") is None
          and report.get("native_codegen", {}).get("backend") == "llvm-api")
    cache = report.get("native_codegen", {}).get("object_cache", {})
    if not isinstance(cache, dict):
        return False
    if plan["cache_mode"] == "incremental":
        ok &= bool(cache.get("enabled")) and (request != "cold_process" or cache.get("hits") == 0)
        ok &= request != "unchanged" or cache.get("hits") == report.get("native_codegen", {}).get("actual_units")
    else:
        ok &= not cache.get("requested") and not cache.get("enabled")
    native = report.get("native_codegen", {})
    mode = native.get("closed_executable", False)
    count = native.get("internalized_functions", 0)
    ok &= (mode is plan["closed_executable"] and type(count) is int and count >= 0
           and (mode or count == 0) and (not mode or "internalized_functions" in native))
    query = report.get("query_cache")
    if query is not None and not isinstance(query, dict):
        return False
    if plan["query_cache"]:
        query = query or {}
        hit = request == "unchanged"
        key = query.get("key")
        ok &= (query.get("requested") is True and query.get("state") == ("hit" if hit else "miss")
               and query.get("stored") is (not hit) and query.get("reason") is None
               and query.get("write_reason") is None and isinstance(key, str) and len(key) == 64
               and all(c in "0123456789abcdef" for c in key)
               and all(type(query.get(field)) is int and query[field] >= 0
                       for field in ("ct_steps", "ct_nodes", "ct_functions")))
    elif query is not None:
        ok &= (query.get("requested") is False and query.get("state") == "disabled"
               and query.get("stored") is False)
    return ok


def one_build(plan, root, case, seq, request, work, logs, tag):
    snapshot, arm = common.SNAPSHOTS[request], seq["arm"]
    common.legacy.sync_sources(root / case["root"] / snapshot / ("rust" if arm == "A" else "zil"), work / "src")
    output = work / "program"
    output.unlink(missing_ok=True); output.with_suffix(".ll").unlink(missing_ok=True)
    argv = command(plan, arm, work)
    row = {**seq, "request": request, "snapshot": snapshot, "work": str(work), "argv": argv,
           "stdout": tag + ".build.stdout", "stderr": tag + ".build.stderr", "checks": [], "pass": False}
    try:
        row["source_before"] = common.file_hashes(work / "src")
        row.update(common.legacy.timed(argv, logs / row["stdout"], logs / row["stderr"], plan["cpus"], plan["timeout_seconds"]))
        row["source_after"] = common.file_hashes(work / "src")
        row.update(stdout_sha256=sha(logs / row["stdout"]), stderr_sha256=sha(logs / row["stderr"]))
        ok = (row["exit"] == 0 and not row["timeout"] and output.is_file()
              and row["source_before"] == row["source_after"] == source_hashes(plan, case, snapshot, arm))
        if arm == "B":
            report = json.loads((logs / row["stdout"]).read_text())
            row["reported"] = {k: report.get(k) for k in ("status", "verification_runs", "native_codegen", "commands", "disk_cache", "query_cache", "timings")}
            ok &= report_ok(plan, request, report)
        if ok:
            row["executable"] = common.inventory(output)
            if arm == "B":
                row["llvm_sha256"] = sha(output.with_suffix(".ll"))
            for index, sample in enumerate(case["inputs"]):
                row["checks"].append(runtime.run_native(plan, root, row["executable"], sample["path"], sample["expected"][snapshot], logs, tag + f".oracle{index}"))
        row["pass"] = ok and len(row["checks"]) == len(case["inputs"]) and all(c["pass"] for c in row["checks"])
    except (OSError, ValueError, KeyError) as error:
        row["error"] = repr(error)
    return row


def execute(plan, root, out, order, warmup=False):
    logs, cases = fresh(out / "logs"), {c["id"]: c for c in plan["cases"]}
    count, failures, artifacts = 0, [], []
    plan_hash = sha(root / "plan.json")
    with (out / "records.jsonl").open("x") as stream:
        for sequence, seq in enumerate(order):
            work = fresh(out / f"work-{sequence:04d}")
            case = cases[seq["case"]]
            for request in plan["request_weights"]:
                try:
                    row = one_build(plan, root, case, seq, request, work, logs, f"{count:05d}")
                    if warmup and request == "cold_process" and row["pass"]:
                        dest = out / "executables" / (seq["case"] + "-" + seq["arm"])
                        dest.parent.mkdir(exist_ok=True)
                        shutil.copy2(work / "program", dest)
                        artifact = {"case": seq["case"], "arm": seq["arm"], "executable": common.inventory(dest)}
                        for kind, source, expected in (("startup", "inputs/empty.bin", "expected/empty.bin"),
                                                      ("warmup", case["input"], case["expected"])):
                            artifact[kind] = runtime.run_native(plan, root, artifact["executable"], source, expected, logs, f"{count:05d}.{kind}")
                        row["pass"] &= artifact["startup"]["pass"] and artifact["warmup"]["pass"]
                        artifacts.append(artifact)
                except Exception as error:
                    row = {**seq, "request": request, "snapshot": common.SNAPSHOTS[request], "pass": False, "checks": [], "runner_error": repr(error)}
                row.update(index=count, plan_sha256=plan_hash, sample_kind="preflight-unscored" if warmup else "build")
                stream.write(json.dumps(row) + "\n"); stream.flush()
                if not row["pass"]:
                    failures.append(count)
                print(count + 1, seq["case"], seq["arm"], request, row["pass"], flush=True)
                count += 1
            shutil.rmtree(work)
    if warmup:
        save(out / "artifacts.json", artifacts)
    return {"plan_sha256": plan_hash, "rows": count, "failures": failures, "pass": not failures,
            "records_sha256": sha(out / "records.jsonl"), "warmup": warmup,
            "artifacts_sha256": sha(out / "artifacts.json") if warmup else None}


def preflight_order(plan):
    return [{"case": case["id"], "arm": arm, "block": 0, "position": pos}
            for case in plan["cases"] for pos, arm in enumerate("AB")]


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def log_ok(logs, row, empty_stderr=False):
    return all(sha(logs / row[stream]) == row[stream + "_sha256"]
               and (not empty_stderr or stream != "stderr" or (logs / row[stream]).stat().st_size == 0)
               for stream in ("stdout", "stderr"))


def native_ok(plan, row, logs, input_path, expected_path, executable=None):
    return (row.get("pass") and row["exit"] == 0 and not row["timeout"]
            and row["input"] == input_path and row["expected"] == expected_path
            and row["input_sha256"] == row["input_after_sha256"] == plan["fixtures_sha256"][input_path]
            and row["stdout_sha256"] == row["expected_stdout_sha256"] == plan["fixtures_sha256"][expected_path]
            and row["executable"] == row["executable_after"]
            and (executable is None or row["executable"] == executable) and log_ok(logs, row, True))


def valid_time(row):
    return all(isinstance(row.get(k), (int, float)) and math.isfinite(row[k]) and row[k] >= 0
               for k in ("wall_seconds", "cpu_user", "cpu_system")) and row["wall_seconds"] > 0


def validate_build_rows(plan, root, records, observed, order, kind):
    expected = [{**seq, "request": request} for seq in order for request in plan["request_weights"]]
    keys = ("block", "case", "position", "arm", "request")
    complete = [tuple(r.get(k) for k in keys) for r in observed] == [tuple(r[k] for k in keys) for r in expected]
    cases, failures, logs = {c["id"]: c for c in plan["cases"]}, [], records.parent / "logs"
    for index, row in enumerate(observed):
        try:
            case, snapshot = cases[row["case"]], common.SNAPSHOTS[row["request"]]
            ok = (row["pass"] and row["exit"] == 0 and not row["timeout"] and row["index"] == index
                  and row["sample_kind"] == kind and row["plan_sha256"] == sha(root / "plan.json")
                  and row["snapshot"] == snapshot and row["argv"] == command(plan, row["arm"], Path(row["work"]))
                  and row["source_before"] == row["source_after"] == source_hashes(plan, case, snapshot, row["arm"])
                  and valid_time(row) and log_ok(logs, row) and len(row["checks"]) == len(case["inputs"]))
            if row["arm"] == "B":
                report = json.loads((logs / row["stdout"]).read_text())
                ok &= report_ok(plan, row["request"], report)
                ok &= row["reported"] == {k: report.get(k) for k in row["reported"]}
            for check, sample in zip(row["checks"], case["inputs"]):
                ok &= native_ok(plan, check, logs, sample["path"], sample["expected"][snapshot], row["executable"])
        except (OSError, KeyError, ValueError, TypeError):
            ok = False
        if not ok:
            failures.append(index)
    return complete, failures


def checked_preflight(plan, root, path):
    receipt, records = json.loads(path.read_text()), path.parent / "records.jsonl"
    observed = rows(records)
    complete, failures = validate_build_rows(plan, root, records, observed, preflight_order(plan), "preflight-unscored")
    if (not receipt.get("pass") or not receipt.get("warmup") or receipt["plan_sha256"] != sha(root / "plan.json")
            or receipt["records_sha256"] != sha(records) or receipt["rows"] != len(observed)
            or receipt["artifacts_sha256"] != sha(path.parent / "artifacts.json") or not complete or failures):
        raise SystemExit("Matching complete successful raw preflight required")
    artifacts = json.loads((path.parent / "artifacts.json").read_text())
    if [(r["case"], r["arm"]) for r in artifacts] != [(c["id"], arm) for c in plan["cases"] for arm in "AB"]:
        raise SystemExit("Incomplete base executable set")
    cases = {c["id"]: c for c in plan["cases"]}
    for artifact in artifacts:
        case, exe = cases[artifact["case"]], artifact["executable"]
        if (common.inventory(exe["path"]) != exe
                or not native_ok(plan, artifact["startup"], path.parent / "logs", "inputs/empty.bin", "expected/empty.bin", exe)
                or not native_ok(plan, artifact["warmup"], path.parent / "logs", case["input"], case["expected"], exe)):
            raise SystemExit("Base executable/startup/warmup identity changed")
    return {(r["case"], r["arm"]): r for r in artifacts}


def preflight(args):
    plan, root = load(args.plan)
    out = fresh(args.out.resolve())
    result = execute(plan, root, out, preflight_order(plan), warmup=True)
    load(args.plan)
    save(out / "result.json", result)


def measure(args):
    plan, root = load(args.plan)
    if not args.quiet_window:
        raise SystemExit("Explicit --quiet-window required; pause other builds/tests")
    artifacts = checked_preflight(plan, root, args.preflight)
    out = fresh(args.out.resolve())
    if args.action == "measure":
        result = execute(plan, root, out, plan["order"])
    else:
        logs, failures = fresh(out / "logs"), []
        cases = {c["id"]: c for c in plan["cases"]}
        with (out / "records.jsonl").open("x") as stream:
            for index, seq in enumerate(plan["order"]):
                case = cases[seq["case"]]
                row = runtime.run_native(plan, root, artifacts[(seq["case"], seq["arm"])]["executable"], case["input"], case["expected"], logs, f"{index:05d}")
                row.update(seq, index=index, sample_kind="runtime", plan_sha256=sha(args.plan), preflight_sha256=sha(args.preflight))
                stream.write(json.dumps(row) + "\n"); stream.flush()
                if not row["pass"]:
                    failures.append(index)
                print(index + 1, seq["case"], seq["arm"], row["pass"], flush=True)
        result = {"plan_sha256": sha(args.plan), "rows": len(plan["order"]), "failures": failures,
                  "pass": not failures, "records_sha256": sha(out / "records.jsonl")}
    # A tool/input mutation at any time in the window invalidates completion.
    load(args.plan)
    result.update(preflight_sha256=sha(args.preflight), sample_kind="build" if args.action == "measure" else "runtime")
    save(out / "complete.json", result)


def summarize(args):
    plan, root = load(args.plan, check_environment=False)
    artifacts = checked_preflight(plan, root, args.preflight)
    observed = rows(args.records)
    if args.kind == "build":
        complete, failures = validate_build_rows(plan, root, args.records, observed, plan["order"], "build")
        aggregate = common.legacy.aggregate
    else:
        keys = ("block", "case", "position", "arm")
        complete = [tuple(r.get(k) for k in keys) for r in observed] == [tuple(r[k] for k in keys) for r in plan["order"]]
        cases, failures = {c["id"]: c for c in plan["cases"]}, []
        for index, row in enumerate(observed):
            try:
                case = cases[row["case"]]
                ok = (row["sample_kind"] == "runtime" and row["index"] == index and valid_time(row)
                      and row["plan_sha256"] == sha(args.plan) and row["preflight_sha256"] == sha(args.preflight)
                      and native_ok(plan, row, args.records.parent / "logs", case["input"], case["expected"], artifacts[(row["case"], row["arm"])]["executable"]))
            except (OSError, KeyError, ValueError, TypeError):
                ok = False
            if not ok:
                failures.append(index)
        aggregate = runtime.aggregate
    try:
        receipt = json.loads((args.records.parent / "complete.json").read_text())
        receipt_ok = (receipt["pass"] and not receipt["failures"] and receipt["plan_sha256"] == sha(args.plan)
                      and receipt["preflight_sha256"] == sha(args.preflight) and receipt["sample_kind"] == args.kind
                      and receipt["rows"] == len(observed) and receipt["records_sha256"] == sha(args.records))
    except (OSError, ValueError, KeyError):
        receipt_ok = False
    result = {"schema": SCHEMA, "plan_sha256": sha(args.plan), "preflight_sha256": sha(args.preflight),
              "records_sha256": sha(args.records), "kind": args.kind, "arm_A": "Rust", "arm_B": "ZIL",
              "source_identity": plan["source_identity"], "versions": plan["versions"], "profile": plan["profile"],
              "cache_mode": plan["cache_mode"], "rust_profile": plan["rust_profile"], "units": plan["units"], "jobs": plan["jobs"],
              "rust_lto": plan["rust_lto"], "rust_lto_scope": plan["rust_lto_scope"],
              "query_cache": plan["query_cache"], "closed_executable": plan["closed_executable"],
              "selected_original_weight": plan["selected_original_weight"], "rows": len(observed), "failures": failures,
              "coverage_complete": complete, "completion_receipt_matches": receipt_ok,
              "scope": plan["scope"] if args.kind == "build" else plan["runtime_scope"],
              "comparison_limits": plan["comparison_limits"], "product_gates": "NotRun", "independent_review": "NotRun"}
    if not complete or not receipt_ok or failures:
        result["verdict"] = "IncompleteOrFailed_NoScore"
        save(args.out, result)
        raise SystemExit("Incomplete, failed or changed rows/logs; no score")
    blocks = list(range(plan["blocks"]))
    result["wall"] = aggregate(plan, observed, blocks, "wall_seconds")
    result["cpu"] = aggregate(plan, observed, blocks, "cpu_seconds")
    rng = random.Random(plan["seed"])
    samples = sorted(aggregate(plan, observed, rng.choices(blocks, k=len(blocks)), "wall_seconds")["B_over_A"] for _ in range(2000))
    result["exploratory_paired_block_bootstrap95"] = [samples[49], samples[1949]]
    result["interval_scope"] = "Shared host, finite paired blocks; descriptive interval, not product confidence or whole-language claim"
    result["verdict"] = "ExploratoryComplete_NotAProductGate"
    save(args.out, result)
    print(json.dumps(result, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare")
    for name in ("out", "zil", "rustc", "clang"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--zil-source", required=True)
    p.add_argument("--tool", type=Path, action="append", required=True, help="Actual LLVM shared library and any additional tool dependency; repeatable")
    p.add_argument("--profile", choices=["O0", "O2"], default="O0")
    p.add_argument("--cache-mode", choices=["cold", "incremental"], default="cold")
    p.add_argument("--query-cache", action="store_true", help="Explicit CP12 trusted-local full-input checkpoints; incremental profile only")
    p.add_argument("--closed-executable", action="store_true", help="Explicit CP13 Linux executable linkage optimization; independently check runtime quality")
    p.add_argument("--rust-profile", choices=["native", "matched"], default="native")
    p.add_argument("--rust-lto", choices=["native", "off"], default="native",
                   help="Keep the native Rust LTO policy (default), or explicitly disable LTO in a separate mechanism diagnostic; runtime must be checked independently")
    p.add_argument("--rust-linker", type=Path, help="Optional explicit Rust driver; omitted means the toolchain default")
    p.add_argument("--units", type=int, choices=range(1, 65), default=1)
    p.add_argument("--jobs", type=int, choices=range(1, 65), default=1)
    p.add_argument("--blocks", type=int, choices=[3, 5], default=3)
    p.add_argument("--case", action="append", help="Freeze a subset for smoke; omitted means all five cases")
    p.add_argument("--cpus"); p.add_argument("--seed", type=int, default=20261030)
    p.add_argument("--timeout", type=int, default=180); p.add_argument("--runtime-timeout", type=int, default=30)
    p.set_defaults(run=prepare)
    for name, handler in (("preflight", preflight), ("measure", measure), ("measure-runtime", measure), ("summarize", summarize)):
        p = sub.add_parser(name)
        p.add_argument("--plan", type=Path, required=True); p.add_argument("--out", type=Path, required=True)
        if name != "preflight":
            p.add_argument("--preflight", type=Path, required=True)
        if name in ("measure", "measure-runtime"):
            p.add_argument("--quiet-window", action="store_true")
        if name == "summarize":
            p.add_argument("--records", type=Path, required=True)
            p.add_argument("--kind", choices=["build", "runtime"], required=True)
        p.set_defaults(run=handler)
    args = parser.parse_args()
    if getattr(args, "timeout", 1) <= 0 or getattr(args, "runtime_timeout", 1) <= 0:
        parser.error("Timeouts must be positive")
    args.run(args)


if __name__ == "__main__":
    main()

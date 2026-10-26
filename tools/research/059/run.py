#!/usr/bin/env python3
"""Freeze/replay research 059 against an explicit schema-8 production archive.

No checkout of today's main is used for Rust path dependencies. Preparation
archives the full production baseline, then overlays only the requested research
commit's tool/fixture/docs paths. No network, installation, or production edits.
"""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import sys
import tarfile
import time

BASE = "6acda08c72dbd3e387893b4260770f0834dcdbe0"
METHODS = ["mono", "cut", "wire", "production"]
OVERLAY = ["tools/research/059", "tests/perf/impl059", "docs/research/059"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
           CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_BUILD_JOBS="1")

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

def checked(command, cwd, log=None, timeout=300):
    started = time.perf_counter_ns()
    try:
        p = subprocess.run(command, cwd=cwd, env=ENV, capture_output=True, timeout=timeout)
        result = dict(command=list(map(str, command)), returncode=p.returncode,
                      stdout=p.stdout.decode(errors="replace"), stderr=p.stderr.decode(errors="replace"),
                      wall_ns=time.perf_counter_ns() - started)
    except subprocess.TimeoutExpired as e:
        result = dict(command=list(map(str, command)), returncode=None, outcome="Timeout",
                      stdout=(e.stdout or b"").decode(errors="replace"),
                      stderr=(e.stderr or b"").decode(errors="replace"), wall_ns=time.perf_counter_ns()-started)
    if log is not None:
        write(log, result)
    if result["returncode"] != 0:
        raise RuntimeError(f"command failed; original receipt {log}: {result['stderr'][-1500:]}")
    return result

def inventory():
    result = dict(available=False, self_visible=False, unreadable=0, names=[],
                  limitation="A namespace process inventory is not proof of isolation; formal isolation comes from the recorded root-coordinated window.")
    try:
        entries = list(Path("/proc").iterdir())
    except OSError as e:
        result["error"] = str(e)
        return result
    result["available"] = True
    for p in entries:
        if p.name.isdigit():
            if int(p.name) == os.getpid():
                result["self_visible"] = True
            try:
                result["names"].append(p.joinpath("comm").read_text().strip())
            except OSError:
                result["unreadable"] += 1
    result["names"].sort()
    return result

def extract(repo, commit, paths, destination):
    data = subprocess.check_output(["git", "archive", "--format=tar", commit, *paths], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            name = Path(member.name)
            if name.is_absolute() or ".." in name.parts or member.issym() or member.islnk():
                raise RuntimeError(f"unexpected archive member: {member.name}")
        archive.extractall(destination, filter="data")

def source_hashes(tree):
    return {str(p.relative_to(tree)): digest(p) for p in sorted(tree.rglob("*")) if p.is_file()}

def prepare(args):
    repo = Path(args.repo).resolve()
    out = Path(args.out).resolve()
    if out.exists():
        raise RuntimeError("new freeze path required; refusing to relabel an existing run")
    candidate = subprocess.check_output(["git", "rev-parse", args.candidate], cwd=repo, text=True).strip()
    out.mkdir(parents=True)
    tree = out / "source"
    tree.mkdir()
    extract(repo, BASE, [], tree)
    extract(repo, candidate, OVERLAY, tree)
    # Bind this orchestrator too: invoking a changed main-branch script cannot
    # silently reuse the old candidate's manifest or sample identity.
    actual_driver = digest(Path(__file__).resolve())
    frozen_driver = digest(tree / "tools/research/059/run.py")
    if actual_driver != frozen_driver:
        raise RuntimeError("execute the requested candidate's exact run.py")
    target = out / "target"
    manifest = tree / "tools/research/059/Cargo.toml"
    identity = dict(source_baseline=BASE, research_commit=candidate, schema=8,
                    driver_sha256=actual_driver, profile="release; codegen-units=1; debug=0; incremental=0; jobs=1",
                    limits=dict(work_per_candidate=100000000, work_per_oracle=100000000, bits=4096,
                                slots=4096, points=20000, bdd_nodes=300000, cache=600000, total_rows=100000),
                    rustc=checked(["rustc", "-vV"], repo)["stdout"],
                    cargo=checked(["cargo", "-V"], repo)["stdout"],
                    platform=dict(system=platform.system(), release=platform.release(), machine=platform.machine(), cpu_count=os.cpu_count()),
                    inventory=inventory(), formal_samples="NotRun", sample_plan=dict(warmups=2, measured=7, order="four-method cyclic rotation; fresh process each sample; OS caches not flushed"))
    write(out / "identity.json", identity)
    checked(["cargo", "test", "--offline", "--locked", "--manifest-path", str(manifest), "--target-dir", str(target), "--", "--nocapture"], tree, out / "test.json")
    checked(["cargo", "clippy", "--offline", "--locked", "--manifest-path", str(manifest), "--target-dir", str(target), "--all-targets", "--", "-D", "warnings"], tree, out / "clippy.json")
    checked(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", str(manifest), "--target-dir", str(target)], tree, out / "build.json")
    identity["binary_sha256"] = digest(target / "release/cut059")
    identity["rlib_sha256"] = {p.name: digest(p) for p in sorted((target / "release/deps").glob("libzil*.rlib"))}
    write(out / "identity.json", identity)
    write(out / "source-manifest.json", source_hashes(tree))
    cases = json.loads((tree / "tools/research/059/cases.json").read_text())["cases"]
    eligibility = []
    for case in cases:
        results = {}
        for method in METHODS:
            dest = out / "freeze" / case["id"] / method
            command = [str(target / "release/cut059"), str(tree / case["path"]), method]
            if method != "production": command.append(str(dest / "relations"))
            receipt = checked(command, tree, dest / "receipt.json", timeout=120)
            data = json.loads(receipt["stdout"])
            write(dest / "data.json", data)
            results[method] = data
            for f in data.get("functions", []):
                if f["outcome"] not in ("Complete", "UnsupportedMultiplicity", "Unsupported", "ResourceLimit", "OracleResourceLimit", "OracleUnsupported"):
                    raise RuntimeError(f"semantic/infrastructure failure: {case['id']} {method} {f}")
        complete = all(all(f["outcome"] == "Complete" for f in results[m]["functions"]) for m in METHODS[:3])
        ids = [[f["id"] for f in results[m]["functions"]] for m in METHODS[:3]]
        if not ids[0] or ids[0] != ids[1] or ids[0] != ids[2] or len(ids[0]) != len(set(ids[0])):
            raise RuntimeError(f"nonempty identical function census required: {case['id']} {ids}")
        eligibility.append(dict(case=case, timed=complete, function_ids=ids[0], methods=METHODS,
                                outcomes={m: [f["outcome"] for f in results[m].get("functions", [])] for m in METHODS}))
    write(out / "eligibility.json", eligibility)
    files = [p for p in sorted((out / "freeze").rglob("*")) if p.is_file()]
    files.append(out / "eligibility.json")
    write(out / "freeze-manifest.json", {str(p.relative_to(out)): digest(p) for p in files})
    print(json.dumps(dict(out=str(out), candidate=candidate, timed_cases=[e["case"]["id"] for e in eligibility if e["timed"]], excluded_cases=[e["case"]["id"] for e in eligibility if not e["timed"]])))

def verify(out):
    identity = json.loads((out / "identity.json").read_text())
    if identity["source_baseline"] != BASE or identity["schema"] != 8:
        raise RuntimeError("frozen source/schema mismatch")
    if digest(Path(__file__).resolve()) != identity["driver_sha256"]:
        raise RuntimeError("driver mismatch")
    if digest(out / "target/release/cut059") != identity["binary_sha256"]:
        raise RuntimeError("binary mismatch")
    for name, expected in json.loads((out / "source-manifest.json").read_text()).items():
        if digest(out / "source" / name) != expected:
            raise RuntimeError(f"source changed: {name}")
    for name, expected in json.loads((out / "freeze-manifest.json").read_text()).items():
        if digest(out / name) != expected:
            raise RuntimeError(f"freeze member changed: {name}")
    return identity

def sample(args):
    out = Path(args.out).resolve()
    identity = verify(out)
    raw = out / "samples.jsonl"
    if raw.exists(): raise RuntimeError("sample path already exists; never append/relabel a prior attempt")
    if not args.window: raise RuntimeError("root-coordinated exclusive window ID required")
    eligible = [e for e in json.loads((out / "eligibility.json").read_text()) if e["timed"]]
    write(out / "sample-start.json", dict(window=args.window, coordinator_assertion="root coordinated all other builds/native stress/measurements idle", inventory=inventory(), identity=identity, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())))
    with raw.open("x") as log:
        for repeat in range(-2, 7):
            for ci, admitted in enumerate(eligible):
                case = admitted["case"]
                offset = (repeat + 2 + ci) % len(METHODS)
                order = METHODS[offset:] + METHODS[:offset]
                for method in order:
                    command = [str(out / "target/release/cut059"), str(out / "source" / case["path"]), method]
                    receipt = checked(command, out / "source", out / "commands" / f"r{repeat}_{case['id']}_{method}.json", timeout=120)
                    data = json.loads(receipt["stdout"])
                    if data.get("method") != method or data.get("schema") != 8:
                        write(out / "sample-failure.json", dict(case=case, method=method, repeat=repeat, receipt=receipt))
                        raise RuntimeError("sample method/schema mismatch")
                    if method != "production" and [f["id"] for f in data["functions"]] != admitted["function_ids"]:
                        write(out / "sample-failure.json", dict(case=case, method=method, repeat=repeat, receipt=receipt))
                        raise RuntimeError("sample function census mismatch")
                    if method != "production" and any(f["outcome"] != "Complete" for f in data["functions"]):
                        write(out / "sample-failure.json", dict(case=case, method=method, repeat=repeat, receipt=receipt))
                        raise RuntimeError("a previously complete function ceased to be Complete; preserve raw attempt")
                    record = dict(case=case["id"], method=method, repeat=repeat, measured=repeat >= 0,
                                  wall_ns=receipt["wall_ns"], data=data)
                    log.write(json.dumps(record, sort_keys=True) + "\n")
                    log.flush()
    summarize(out)
    write(out / "sample-finish.json", dict(window=args.window, raw_sha256=digest(raw), summary_sha256=digest(out / "summary.json"), utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), inventory=inventory()))
    print(json.dumps(dict(out=str(out), raw_sha256=digest(raw), measured=len(eligible)*4*7, warmups=len(eligible)*4*2)))

def summarize(out):
    records = [json.loads(line) for line in (out / "samples.jsonl").read_text().splitlines()]
    measured = [r for r in records if r["measured"]]
    groups = {}
    for r in measured:
        for metric in ("wall_ns", "total_ns", "frontend_ns", "candidate_total_ns", "oracle_total_ns", "production_ns"):
            value = r.get(metric) if metric == "wall_ns" else r["data"].get(metric)
            if value is not None: groups.setdefault((r["case"], "module", r["method"], metric), []).append(value)
        for f in r["data"].get("functions", []):
            for metric, value in f.items():
                if metric.endswith("_ns") or metric in ("candidate_work", "oracle_work", "rows", "allocated_bdd_nodes", "max_point_support", "max_point_reachable_bdd_nodes", "max_transition_plan_count", "max_factor_support", "wire_substitutions", "materialized_bytes"):
                    groups.setdefault((r["case"], f"f{f['id']}:{f['name']}", r["method"], metric), []).append(value)
    stats = []
    for (case, scope, method, metric), values in sorted(groups.items()):
        if len(values) != 7: raise RuntimeError("missing/duplicate statistic samples")
        stats.append(dict(case=case, scope=scope, method=method, metric=metric, count=len(values),
                          median=statistics.median(values), minimum=min(values), maximum=max(values)))
    ratios = []
    rng = random.Random(59)
    for (case, scope, method, metric), values in sorted(groups.items()):
        if method not in ("cut", "wire") or not metric.endswith("_ns"): continue
        base = groups.get((case, scope, "mono", metric))
        if base is None or statistics.median(base) == 0: continue
        samples = []
        for _ in range(1000):
            indices = [rng.randrange(7) for _ in range(7)]
            denominator = statistics.median([base[i] for i in indices])
            if denominator: samples.append(statistics.median([values[i] for i in indices]) / denominator)
        samples.sort()
        ratios.append(dict(case=case, scope=scope, method=method, metric=metric,
                           ratio_of_medians=statistics.median(values)/statistics.median(base),
                           paired_bootstrap_95=[samples[int(len(samples)*.025)], samples[min(len(samples)-1, int(len(samples)*.975))]]))
    write(out / "summary.json", dict(measured=len(measured), warmups=len(records)-len(measured),
                                     statistics=stats, ratios=ratios, bootstrap="1000 deterministic paired resamples, seed59; exploratory small-sample uncertainty, not a production claim"))

def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="command", required=True)
    f = sub.add_parser("prepare"); f.add_argument("--repo", required=True); f.add_argument("--candidate", required=True); f.add_argument("--out", required=True)
    s = sub.add_parser("sample"); s.add_argument("--out", required=True); s.add_argument("--window", required=True)
    q = sub.add_parser("verify"); q.add_argument("--out", required=True)
    r = sub.add_parser("summarize"); r.add_argument("--out", required=True)
    args = p.parse_args()
    if args.command == "prepare": prepare(args)
    elif args.command == "sample": sample(args)
    elif args.command == "verify": print(json.dumps(verify(Path(args.out).resolve()), indent=2))
    else: summarize(Path(args.out).resolve())

if __name__ == "__main__":
    main()

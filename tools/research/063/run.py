#!/usr/bin/env python3
"""Freeze schema-9 graph experiments and a one-patch actual driver comparison.

Run prepare before asking root for an exclusive window. Sample builds nothing.
An incomplete candidate is retained but never included in successful ratios.
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
import re
import shutil
import statistics
import subprocess
import tarfile
import time

BASE = "70d3a729a6db6f8355ea99942e6f05207eb8815b"
OVERLAY = ["tools/research/063", "tests/perf/impl063", "docs/research/063", "changes/IMPLEMENTATION-0063.md"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_BUILD_JOBS="1")
ENV.pop("ZIL_063_DIAGNOSTICS", None)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def invoke(command, cwd, log, timeout=300, stdin=None, extra_env=None):
    start = time.perf_counter_ns()
    try:
        p = subprocess.run(list(map(str, command)), cwd=cwd, env=ENV | (extra_env or {}), input=stdin, capture_output=True, timeout=timeout)
        value = dict(command=list(map(str, command)), cwd=str(cwd), returncode=p.returncode,
                     stdout=p.stdout.decode(errors="replace"), stderr=p.stderr.decode(errors="replace"),
                     wall_ns=time.perf_counter_ns() - start)
    except subprocess.TimeoutExpired as e:
        value = dict(command=list(map(str, command)), cwd=str(cwd), returncode=None, outcome="Timeout",
                     stdout=(e.stdout or b"").decode(errors="replace"), stderr=(e.stderr or b"").decode(errors="replace"),
                     wall_ns=time.perf_counter_ns() - start)
    if log is not None:
        write(log, value)
    return value


def checked(command, cwd, log=None, timeout=300, **kw):
    value = invoke(command, cwd, log, timeout=timeout, **kw)
    if value["returncode"] != 0:
        raise RuntimeError(f"command failed, preserved {log}: {value['stderr'][-1500:]}")
    return value


def inventory():
    result = dict(available=False, self_visible=False, unreadable=0, names=[], limitation="Namespace inventory is not proof of isolation; the root-coordinated window is required.")
    try:
        entries = list(Path("/proc").iterdir())
    except OSError as e:
        result["error"] = str(e)
        return result
    result["available"] = True
    for p in entries:
        if p.name.isdigit():
            result["self_visible"] |= int(p.name) == os.getpid()
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


def hashes(tree):
    return {str(p.relative_to(tree)): digest(p) for p in sorted(tree.rglob("*")) if p.is_file()}


def graph_freeze(out, cases):
    eligibility = []
    for case in cases["cases"]:
        methods = case.get("methods", cases["methods"])
        common_ids = None
        outcomes, admitted = {}, []
        for method in methods:
            dest = out / "freeze/graph" / case["id"] / method
            command = [out / "target/release/cut063", out / "source" / case["path"], method]
            if method != "production":
                command.append(dest / "relations")
            receipt = checked(command, out / "source", dest / "command.json", timeout=120)
            data = json.loads(receipt["stdout"])
            write(dest / "data.json", data)
            if data["schema"] != 9 or data["method"] != method:
                raise RuntimeError("wrong graph source/method identity")
            if method == "production":
                outcomes[method] = data["outcome"]
                if data["outcome"] not in ("Accepted", "Rejected"):
                    raise RuntimeError("unexpected production outcome")
                admitted.append(method)
                continue
            ids = [f["id"] for f in data["functions"]]
            if not ids or len(ids) != len(set(ids)) or common_ids is not None and common_ids != ids:
                raise RuntimeError(f"inconsistent/nonexistent function census {case['id']}/{method}")
            common_ids = ids
            outcomes[method] = [f["outcome"] for f in data["functions"]]
            for f in data["functions"]:
                if f["outcome"] not in ("Complete", "UnsupportedMultiplicity", "Unsupported", "ResourceLimit", "OracleResourceLimit", "OracleUnsupported"):
                    raise RuntimeError(f"semantic/infrastructure failure: {case['id']} {method} {f}")
                if f["outcome"] == "Complete":
                    stem = dest / "relations" / f"f{f['id']}"
                    if Path(str(stem) + ".json").read_bytes() != Path(str(stem) + ".oracle.json").read_bytes():
                        raise RuntimeError("canonical full candidate/oracle bytes differ")
            if all(f["outcome"] == "Complete" for f in data["functions"]):
                admitted.append(method)
        eligibility.append(dict(case=case, function_ids=common_ids, outcomes=outcomes, methods=admitted if case["timing"] else []))
    write(out / "eligibility.json", eligibility)



def diagnostic_freeze(out):
    methods = ["mono", "cut", "wire", "fused", "fused:owner", "fused:token", "mono:canonical:gc", "cut:canonical:gc", "wire:canonical:gc", "fused:canonical:gc", "fused:owner:gc", "fused:token:gc"]
    for method in methods:
        dest = out / "freeze/diagnostic" / method
        receipt = checked([out / "target/release/cut063", out / "source/tests/perf/impl059/independent.zil", method, dest / "relations"], out / "source", dest / "command.json", extra_env={"ZIL_063_DIAGNOSTICS":"1"})
        data = json.loads(receipt["stdout"])
        write(dest / "data.json", data)
        for f in data["functions"]:
            if f["outcome"] not in ("Complete", "ResourceLimit"):
                raise RuntimeError("diagnostic query failed; preserve its raw error")
            if f["outcome"] == "Complete":
                a, b = dest / "relations" / f"f{f['id']}.json", dest / "relations" / f"f{f['id']}.oracle.json"
                if a.read_bytes() != b.read_bytes():
                    raise RuntimeError("diagnostic canonical output mismatch")

def pipeline_command(out, variant, case, stage, destination):
    target = "target" if variant == "baseline" else "lazy-target"
    return [out / target / "release/pipeline063", destination, stage, *[out / "source" / p for p in case["files"]]]


def stable_report(record):
    report = record["report"]
    return {k: report[k] for k in ("status", "phase", "diagnostics", "input_fingerprint", "cache_hit", "disk_cache", "query_events", "verification_runs", "source_files", "scalar_interfaces")}


def pipeline_freeze(out, cases):
    comparison = []
    for case in cases["pipeline"]:
        results = {}
        for variant in ("baseline", "lazy"):
            dest = out / "freeze/pipeline" / case["id"] / variant
            receipt = checked(pipeline_command(out, variant, case, "build-emit", dest / "run"), out / "source", dest / "command.json")
            data = json.loads(receipt["stdout"])
            native = []
            for record in data["records"]:
                scenario = record["scenario"]
                expected = case.get("private_exit", case["exit"]) if scenario == "private_body_edit" else case["exit"]
                run = invoke([dest / "run" / scenario], out / "source", dest / f"native-{scenario}.json", stdin=case["stdin"].encode())
                if run["returncode"] != expected or run["stderr"]:
                    raise RuntimeError(f"native behavior mismatch {case['id']} {variant} {scenario}")
                native.append({k: run[k] for k in ("returncode", "stdout", "stderr")})
            results[variant] = (data, native)
        left, right = results["baseline"], results["lazy"]
        if [stable_report(r) for r in left[0]["records"]] != [stable_report(r) for r in right[0]["records"]] or left[1] != right[1]:
            raise RuntimeError(f"driver report/native behavior differs: {case['id']}")
        pairs = []
        for a, b in zip(left[0]["records"], right[0]["records"], strict=True):
            aa, bb = a["report"]["artifacts"], b["report"]["artifacts"]
            if aa.keys() != bb.keys():
                raise RuntimeError("artifact census mismatch")
            for name in aa:
                # Executables and publication sidecars are checked by behavior;
                # every requested textual stage dump must be exactly identical.
                if name not in ("executable", "sidecar"):
                    if Path(aa[name]).read_bytes() != Path(bb[name]).read_bytes():
                        raise RuntimeError(f"emitted bytes differ: {case['id']} {a['scenario']} {name}")
                    pairs.append(dict(scenario=a["scenario"], artifact=name, sha256=digest(aa[name])))
        comparison.append(dict(case=case["id"], result="IdenticalReportsNativeBehaviorAndDumpBytes", dump_pairs=pairs, native=left[1]))
    write(out / "pipeline-equivalence.json", comparison)


def prepare(args):
    repo, out = Path(args.repo).resolve(), Path(args.out).resolve()
    if out.exists():
        raise RuntimeError("new path required; prior attempts may not be relabelled")
    candidate = subprocess.check_output(["git", "rev-parse", args.candidate], cwd=repo, text=True).strip()
    out.mkdir(parents=True)
    tree = out / "source"
    tree.mkdir()
    extract(repo, BASE, [], tree)
    extract(repo, candidate, OVERLAY, tree)
    if digest(Path(__file__).resolve()) != digest(tree / "tools/research/063/run.py"):
        raise RuntimeError("invoke the exact requested candidate's runner")
    cases = json.loads((tree / "tools/research/063/cases.json").read_text())
    if cases["source_baseline"] != BASE or cases["schema"] != 9:
        raise RuntimeError("case identity mismatch")
    identity = dict(source_baseline=BASE, research_commit=candidate, schema=9, driver_sha256=digest(Path(__file__).resolve()),
                    profile="release; codegen-units=1; debug=0; incremental=0; jobs=1; no diagnostic census in timing",
                    limits=dict(work_candidate=100000000, work_oracle=100000000, physical_bdd_slots=300000, cache=600000, rows=100000, bits=4096, points=20000),
                    collection="optional safe-boundary recycling; total created nodes may exceed the unchanged physical slot cap; all GC work charged",
                    rustc=checked(["rustc", "-vV"], repo)["stdout"], cargo=checked(["cargo", "-V"], repo)["stdout"],
                    clang=checked([ENV.get("ZIL_CLANG", "clang"), "--version"], repo)["stdout"], clang_sha256=digest(ENV["ZIL_CLANG"]),
                    platform=dict(system=platform.system(), release=platform.release(), machine=platform.machine(), cpu_count=os.cpu_count()),
                    inventory=inventory(), sample_plan=cases["sampling"], formal_samples="NotRun")
    write(out / "identity.json", identity)
    manifest = tree / "tools/research/063/Cargo.toml"
    checked(["cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target", "--", "--nocapture"], tree, out / "test.json", extra_env={"ZIL_063_TEST_EVIDENCE":str(out / "freeze/counterexamples")})
    repeated = checked(["cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target", "--bin", "cut063", "tests::collection_scanning_and_rebuilding_share_the_complete_candidate_budget", "--", "--exact", "--nocapture"], tree, out / "budget-new-process.json")
    first = json.loads((out / "test.json").read_text())
    before = re.findall(r"063 deterministic GC budget n=(\d+)", first["stdout"])
    after = re.findall(r"063 deterministic GC budget n=(\d+)", repeated["stdout"])
    if len(before) != 1 or before != after:
        raise RuntimeError("complete GC work differs across fresh processes")
    checked(["cargo", "clippy", "--offline", "--locked", "--manifest-path", manifest, "--target-dir", out / "target", "--all-targets", "--", "-D", "warnings"], tree, out / "clippy.json")
    checked(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target"], tree, out / "build.json")
    lazy = out / "lazy-source"
    shutil.copytree(tree, lazy)
    patch = tree / "tools/research/063/lazy-dump.patch"
    checked(["patch", "-p1", "--batch", "--forward", "-i", patch], lazy, out / "patch.json")
    modified = [p for p in hashes(tree) if digest(tree / p) != digest(lazy / p)]
    if sorted(modified) != ["crates/zil-driver/src/artifacts.rs", "crates/zil-driver/src/project.rs", "crates/zil-driver/src/session.rs"]:
        raise RuntimeError(f"unexpected lazy patch write set {modified}")
    checked(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", lazy / "tools/research/063/Cargo.toml", "--target-dir", out / "lazy-target", "--bin", "pipeline063"], lazy, out / "lazy-build.json")
    identity["patch_sha256"] = digest(patch)
    identity["binaries"] = {p: digest(out / p) for p in ("target/release/cut063", "target/release/pipeline063", "lazy-target/release/pipeline063")}
    identity["rlibs"] = {str(p.relative_to(out)): digest(p) for target in ("target", "lazy-target") for p in sorted((out / target / "release/deps").glob("libzil*.rlib"))}
    write(out / "identity.json", identity)
    write(out / "source-manifest.json", hashes(tree))
    write(out / "lazy-source-manifest.json", hashes(lazy))
    graph_freeze(out, cases)
    diagnostic_freeze(out)
    pipeline_freeze(out, cases)
    members = [p for p in sorted((out / "freeze").rglob("*")) if p.is_file()]
    members += [out / "eligibility.json", out / "pipeline-equivalence.json"]
    write(out / "freeze-manifest.json", {str(p.relative_to(out)): digest(p) for p in members})
    estimated = 0
    for e in json.loads((out / "eligibility.json").read_text()):
        for method in e["methods"]:
            estimated += json.loads((out / "freeze/graph" / e["case"]["id"] / method / "command.json").read_text())["wall_ns"] * 9
    print(json.dumps(dict(out=str(out), candidate=candidate, estimated_graph_seconds=estimated/1e9, note="canonical output I/O adds freeze overhead; pipeline timing additional")))


def verify(out):
    identity = json.loads((out / "identity.json").read_text())
    if identity["source_baseline"] != BASE or identity["schema"] != 9 or digest(Path(__file__).resolve()) != identity["driver_sha256"]:
        raise RuntimeError("frozen driver/source identity mismatch")
    for name, value in identity["binaries"].items():
        if digest(out / name) != value:
            raise RuntimeError(f"binary changed {name}")
    if digest(ENV["ZIL_CLANG"]) != identity["clang_sha256"]:
        raise RuntimeError("clang changed")
    for folder, manifest in (("source", "source-manifest.json"), ("lazy-source", "lazy-source-manifest.json"), ("", "freeze-manifest.json")):
        for name, value in json.loads((out / manifest).read_text()).items():
            if digest(out / folder / name) != value:
                raise RuntimeError(f"frozen member changed {folder}/{name}")
    return identity


def sample(args):
    out = Path(args.out).resolve()
    identity = verify(out)
    raw = out / "samples.jsonl"
    if raw.exists() or not args.window:
        raise RuntimeError("new samples and root-coordinated exclusive window required")
    cases = json.loads((out / "source/tools/research/063/cases.json").read_text())
    eligibility = json.loads((out / "eligibility.json").read_text())
    write(out / "sample-start.json", dict(window=args.window, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), coordinator_assertion="root coordinated other builds, tests and timing idle", identity=identity, inventory=inventory()))
    with raw.open("x") as log:
        for repeat in range(-2, 7):
            for ci, entry in enumerate(eligibility):
                methods = entry["methods"]
                if not methods:
                    continue
                offset = (repeat + 2 + ci) % len(methods)
                for method in methods[offset:] + methods[:offset]:
                    case = entry["case"]
                    receipt = checked([out / "target/release/cut063", out / "source" / case["path"], method], out / "source", out / "commands/graph" / f"r{repeat}_{case['id']}_{method}.json", timeout=120)
                    data = json.loads(receipt["stdout"])
                    if data["schema"] != 9 or data["method"] != method:
                        raise RuntimeError("sample method/schema mismatch")
                    if method == "production":
                        if data["outcome"] != entry["outcomes"][method]:
                            raise RuntimeError("production disposition changed")
                    elif [f["id"] for f in data["functions"]] != entry["function_ids"] or any(f["outcome"] != "Complete" for f in data["functions"]):
                        raise RuntimeError("previously complete function no longer complete; raw attempt retained")
                    log.write(json.dumps(dict(kind="graph", case=case["id"], method=method, repeat=repeat, measured=repeat>=0, wall_ns=receipt["wall_ns"], data=data), sort_keys=True) + "\n")
                    log.flush()
            for ci, case in enumerate(cases["pipeline"]):
                for stage in cases["sampling"]["stages"]:
                    variants = ["baseline", "lazy"] if (repeat+ci)%2 == 0 else ["lazy", "baseline"]
                    pair = {}
                    for variant in variants:
                        dest = out / "pipeline-samples" / f"r{repeat}_{case['id']}_{stage}_{variant}"
                        receipt = checked(pipeline_command(out, variant, case, stage, dest), out / "source", out / "commands/pipeline" / f"r{repeat}_{case['id']}_{stage}_{variant}.json")
                        data = json.loads(receipt["stdout"])
                        pair[variant] = [stable_report(r) for r in data["records"]]
                        log.write(json.dumps(dict(kind="pipeline", case=case["id"], stage=stage, method=variant, repeat=repeat, measured=repeat>=0, wall_ns=receipt["wall_ns"], data=data), sort_keys=True) + "\n")
                        log.flush()
                    if pair["baseline"] != pair["lazy"]:
                        raise RuntimeError("paired driver reports differ")
    rust_profiles(out)
    summarize(out)
    write(out / "sample-finish.json", dict(window=args.window, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), raw_sha256=digest(raw), summary_sha256=digest(out / "summary.json"), inventory=inventory()))
    print(json.dumps(dict(out=str(out), raw_sha256=digest(raw))))


def rust_profiles(out):
    # Diagnostic compiler-internal time-passes only: nested intervals cannot be
    # summed as disjoint phases. RUSTC_BOOTSTRAP is explicit and not a release claim.
    for case in ("package_ui", "error_cleanup"):
        for opt in (0, 2):
            dest = out / "rust-profile" / f"{case}-O{opt}"
            dest.mkdir(parents=True)
            command = ["rustc", "--edition=2024", "-Ztime-passes", f"-Copt-level={opt}", "-Cdebuginfo=0", "-Ccodegen-units=1", out / "source/examples/mainline" / case / "main.rs", "-o", dest / "native"]
            checked(command, out / "source", dest / "command.json", extra_env={"RUSTC_BOOTSTRAP":"1"})


def summarize(out):
    records = [json.loads(line) for line in (out / "samples.jsonl").read_text().splitlines()]
    groups = {}
    def add(key, value):
        if type(value) in (int, float):
            groups.setdefault(key, []).append(value)
    for r in records:
        if not r["measured"]:
            continue
        if r["kind"] == "graph":
            for key, value in r["data"].items():
                if key.endswith("_ns"):
                    add(("graph", r["case"], "module", r["method"], key), value)
            add(("graph", r["case"], "process", r["method"], "wall_ns"), r["wall_ns"])
            for f in r["data"].get("functions", []):
                for metric, value in f.items():
                    if metric.endswith("_ns") or metric in ("candidate_work", "oracle_work", "rows", "allocated_bdd_nodes", "occupied_bdd_nodes", "created_bdd_nodes", "gc_count", "gc_collected_nodes", "gc_work", "max_point_reachable_bdd_nodes", "materialized_bytes"):
                        add(("graph", r["case"], f"f{f['id']}:{f['name']}", r["method"], metric), value)
        else:
            add(("pipeline", r["case"], r["stage"]+"/process", r["method"], "wall_ns"), r["wall_ns"])
            for record in r["data"]["records"]:
                add(("pipeline", r["case"], r["stage"]+"/"+record["scenario"], r["method"], "wall_ns"), record["wall_ns"])
    stats, ratios = [], []
    for key, values in sorted(groups.items()):
        if len(values) != 7:
            raise RuntimeError(f"incomplete/duplicate group {key}")
        stats.append(dict(kind=key[0], case=key[1], scope=key[2], method=key[3], metric=key[4], count=7, median=statistics.median(values), minimum=min(values), maximum=max(values)))
    controls = {"cut":"mono", "wire":"mono", "fused":"cut", "mono:canonical:gc":"mono", "cut:canonical:gc":"cut", "wire:canonical:gc":"wire", "fused:canonical:gc":"fused", "mono:owner:gc":"mono:canonical:gc", "cut:owner:gc":"mono:owner:gc", "wire:owner:gc":"mono:owner:gc", "fused:owner:gc":"mono:owner:gc", "fused:token:gc":"fused:owner:gc", "lazy":"baseline"}
    rng = random.Random(63)
    for key, values in sorted(groups.items()):
        control = controls.get(key[3])
        other = groups.get((*key[:3], control, key[4]))
        if not key[4].endswith("_ns") or other is None or statistics.median(other) == 0:
            continue
        boots = []
        for _ in range(1000):
            indices = [rng.randrange(7) for _ in range(7)]
            denominator = statistics.median([other[i] for i in indices])
            if denominator:
                boots.append(statistics.median([values[i] for i in indices])/denominator)
        boots.sort()
        ratios.append(dict(kind=key[0], case=key[1], scope=key[2], method=key[3], control=control, metric=key[4], ratio_of_medians=statistics.median(values)/statistics.median(other), paired_bootstrap_95=[boots[int(len(boots)*.025)], boots[min(len(boots)-1, int(len(boots)*.975))]]))
    write(out / "summary.json", dict(measured=sum(r["measured"] for r in records), warmups=sum(not r["measured"] for r in records), statistics=stats, ratios=ratios, bootstrap="1000 deterministic paired resamples seed63; exploratory small-sample uncertainty, not a product claim"))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="command", required=True)
    f = sub.add_parser("prepare")
    f.add_argument("--repo", required=True); f.add_argument("--candidate", required=True); f.add_argument("--out", required=True)
    s = sub.add_parser("sample"); s.add_argument("--out", required=True); s.add_argument("--window", required=True)
    q = sub.add_parser("verify"); q.add_argument("--out", required=True)
    r = sub.add_parser("summarize"); r.add_argument("--out", required=True)
    args = p.parse_args()
    if args.command == "prepare": prepare(args)
    elif args.command == "sample": sample(args)
    elif args.command == "verify": print(json.dumps(verify(Path(args.out).resolve())))
    else: summarize(Path(args.out).resolve())

if __name__ == "__main__":
    main()

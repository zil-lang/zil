#!/usr/bin/env python3
"""Freeze and replay the schema-10 exact location-relation experiment.

Preparation is functional only. Sampling requires the root's exclusive window;
all non-Complete outcomes remain visible and are excluded from speed ratios.
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
import tarfile
import time

BASE = "400a082444275e1d0e2ad78c4910b9b6fc526be5"
RESEARCH_BASE = "bf5876bd8b5e9936ad52e8f24adaa673884232dc"
OVERLAY = ["tools/research/071", "tests/perf/impl071", "docs/research/071", "changes/IMPLEMENTATION-0071.md"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_BUILD_JOBS="1")
for name in ("CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
    ENV.pop(name, None)
ENV.pop("ZIL_071_DIAGNOSTICS", None)
ENV.pop("ZIL_071_TEST_EVIDENCE", None)
ENV.pop("ZIL_063_TEST_EVIDENCE", None)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def invoke(command, cwd, log=None, timeout=300, extra_env=None):
    start = time.perf_counter_ns()
    try:
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=ENV | (extra_env or {}), capture_output=True, timeout=timeout)
        value = dict(command=list(map(str, command)), cwd=str(cwd), returncode=result.returncode, stdout=result.stdout.decode(errors="replace"), stderr=result.stderr.decode(errors="replace"), wall_ns=time.perf_counter_ns()-start)
    except subprocess.TimeoutExpired as error:
        value = dict(command=list(map(str, command)), cwd=str(cwd), returncode=None, outcome="Timeout", stdout=(error.stdout or b"").decode(errors="replace"), stderr=(error.stderr or b"").decode(errors="replace"), wall_ns=time.perf_counter_ns()-start)
    if log is not None:
        write(log, value)
    if value["returncode"] != 0:
        raise RuntimeError(f"command failed; preserved {log}: {value['stderr'][-1500:]}")
    return value


def inventory():
    result = dict(available=False, self_visible=False, unreadable=0, names=[], limitation="Namespace inventory cannot prove isolation; root must coordinate the window.")
    try:
        entries = list(Path("/proc").iterdir())
    except OSError as error:
        result["error"] = str(error)
        return result
    result["available"] = True
    for entry in entries:
        if entry.name.isdigit():
            result["self_visible"] |= int(entry.name) == os.getpid()
            try:
                result["names"].append(entry.joinpath("comm").read_text().strip())
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
                raise RuntimeError(f"unexpected archive member {member.name}")
        archive.extractall(destination, filter="data")


def hashes(tree):
    return {str(path.relative_to(tree)): digest(path) for path in sorted(tree.rglob("*")) if path.is_file()}


def freeze_graph(out, cases):
    eligibility = []
    for case in cases["cases"]:
        outcomes, admitted, common_ids, baseline_bytes = {}, [], None, {}
        for method in case.get("methods", cases["methods"]):
            destination = out / "freeze/graph" / case["id"] / method
            receipt = invoke([out / "target/release/cut071", out / "source" / case["path"], method, destination / "relations"], out / "source", destination / "command.json", timeout=120)
            data = json.loads(receipt["stdout"])
            write(destination / "data.json", data)
            if data["schema"] != 10 or data["method"] != method:
                raise RuntimeError("wrong query schema/method")
            ids = [function["id"] for function in data["functions"]]
            if not ids or len(ids) != len(set(ids)) or common_ids is not None and common_ids != ids:
                raise RuntimeError("inconsistent function census")
            common_ids = ids
            outcomes[method] = [function["outcome"] for function in data["functions"]]
            for function in data["functions"]:
                if function["outcome"] not in ("Complete", "UnsupportedMultiplicity", "Unsupported", "ResourceLimit", "OracleResourceLimit", "OracleUnsupported"):
                    raise RuntimeError(f"semantic/infrastructure failure: {case['id']} {method} {function}")
                if function["outcome"] in ("Complete", "UnsupportedMultiplicity"):
                    stem = destination / "relations" / f"f{function['id']}"
                    candidate = Path(str(stem) + ".json").read_bytes()
                    oracle = Path(str(stem) + ".oracle.json").read_bytes()
                    if candidate != oracle:
                        raise RuntimeError("canonical full candidate/oracle bytes differ")
                    if function["id"] in baseline_bytes and baseline_bytes[function["id"]] != candidate:
                        raise RuntimeError("canonical output differs between representations or methods")
                    baseline_bytes[function["id"]] = candidate
            if all(function["outcome"] == "Complete" for function in data["functions"]):
                admitted.append(method)
        eligibility.append(dict(case=case, function_ids=common_ids, outcomes=outcomes, complete_methods=admitted, methods=admitted if case["timing"] else []))
    write(out / "eligibility.json", eligibility)


def prepare(args):
    repo, out = Path(args.repo).resolve(), Path(args.out).resolve()
    if out.exists():
        raise RuntimeError("new output directory required; preserve prior attempts")
    candidate = subprocess.check_output(["git", "rev-parse", args.candidate], cwd=repo, text=True).strip()
    out.mkdir(parents=True)
    tree = out / "source"
    tree.mkdir()
    extract(repo, BASE, [], tree)
    extract(repo, RESEARCH_BASE, ["tools/research/067", "tests/perf/impl067"], tree)
    extract(repo, candidate, OVERLAY, tree)
    if digest(Path(__file__).resolve()) != digest(tree / "tools/research/071/run.py"):
        raise RuntimeError("invoke the exact frozen candidate runner")
    cases = json.loads((tree / "tools/research/071/cases.json").read_text())
    if cases["source_baseline"] != BASE or cases["research_baseline"] != RESEARCH_BASE or cases["schema"] != 10:
        raise RuntimeError("case identity mismatch")
    identity = dict(source_baseline=BASE, research_baseline=RESEARCH_BASE, research_commit=candidate, schema=10, runner_sha256=digest(Path(__file__).resolve()), profile="release; codegen-units=1; debug=0; incremental=0; jobs=1; no diagnostic census in timing", limits=dict(work_candidate=100000000, work_oracle=100000000, physical_bdd_slots=300000, cache=600000, rows=100000, bits=4096, points=20000), rustc=invoke(["rustc", "-vV"], repo)["stdout"], cargo=invoke(["cargo", "-V"], repo)["stdout"], platform=dict(system=platform.system(), release=platform.release(), machine=platform.machine(), cpu_count=os.cpu_count()), inventory=inventory(), sample_plan=cases["sampling"], formal_samples="NotRun")
    write(out / "identity.json", identity)
    manifest = tree / "tools/research/071/Cargo.toml"
    invoke(["cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target", "--", "--test-threads=1", "--nocapture"], tree, out / "test.json", extra_env={"ZIL_071_TEST_EVIDENCE":str(out / "freeze/counterexamples071"), "ZIL_063_TEST_EVIDENCE":str(out / "freeze/counterexamples063")})
    invoke(["cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target", "--", "tests::sparse_full_materialization_obeys_the_same_exact_work_limit", "--exact", "--nocapture"], tree, out / "budget-new-process.json")
    import re
    first = re.findall(r"071 exact sparse complete budget n=(\d+)", json.loads((out / "test.json").read_text())["stdout"])
    again = re.findall(r"071 exact sparse complete budget n=(\d+)", json.loads((out / "budget-new-process.json").read_text())["stdout"])
    if len(first) != 1 or first != again:
        raise RuntimeError("complete sparse work differs across fresh processes")
    invoke(["cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target", "--", "tests::direct_local_complete_budget_is_deterministic_and_exact", "--exact", "--nocapture"], tree, out / "direct-budget-new-process.json")
    direct_first = re.findall(r"071 exact direct complete budget n=(\d+)", json.loads((out / "test.json").read_text())["stdout"])
    direct_again = re.findall(r"071 exact direct complete budget n=(\d+)", json.loads((out / "direct-budget-new-process.json").read_text())["stdout"])
    if len(direct_first) != 1 or direct_first != direct_again:
        raise RuntimeError("direct complete work differs across fresh processes")
    invoke(["cargo", "clippy", "--offline", "--locked", "--manifest-path", manifest, "--target-dir", out / "target", "--all-targets", "--", "-D", "warnings"], tree, out / "clippy.json")
    invoke(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", manifest, "--target-dir", out / "target"], tree, out / "build.json")
    identity["binaries"] = {"target/release/cut071": digest(out / "target/release/cut071")}
    identity["rlibs"] = {str(path.relative_to(out)):digest(path) for path in sorted((out / "target/release/deps").glob("libzil*.rlib"))}
    write(out / "identity.json", identity)
    write(out / "source-manifest.json", hashes(tree))
    freeze_graph(out, cases)
    members = [path for path in sorted((out / "freeze").rglob("*")) if path.is_file()] + [out / "eligibility.json"]
    write(out / "freeze-manifest.json", {str(path.relative_to(out)):digest(path) for path in members})
    estimate = sum(json.loads((out / "freeze/graph" / entry["case"]["id"] / method / "command.json").read_text())["wall_ns"] * 9 for entry in json.loads((out / "eligibility.json").read_text()) for method in entry["methods"])
    print(json.dumps(dict(out=str(out), candidate=candidate, estimated_sample_seconds=estimate/1e9, note="functional preparation timings are not formal performance evidence")))


def verify(out):
    identity = json.loads((out / "identity.json").read_text())
    if identity["source_baseline"] != BASE or identity["research_baseline"] != RESEARCH_BASE or identity["schema"] != 10 or digest(Path(__file__).resolve()) != identity["runner_sha256"]:
        raise RuntimeError("frozen identity mismatch")
    for name, value in identity["binaries"].items():
        if digest(out / name) != value:
            raise RuntimeError(f"binary changed {name}")
    for folder, manifest in (("source", "source-manifest.json"), ("", "freeze-manifest.json")):
        for name, value in json.loads((out / manifest).read_text()).items():
            if digest(out / folder / name) != value:
                raise RuntimeError(f"frozen member changed {folder}/{name}")
    return identity


def sample(args):
    out = Path(args.out).resolve()
    identity = verify(out)
    raw = out / "samples.jsonl"
    if raw.exists() or not args.window:
        raise RuntimeError("new sample path and root-coordinated window required")
    eligibility = json.loads((out / "eligibility.json").read_text())
    write(out / "sample-start.json", dict(window=args.window, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), coordinator_assertion="root coordinated other builds, tests and timing idle", identity=identity, inventory=inventory()))
    with raw.open("x") as log:
        for repeat in range(-2, 7):
            for index, entry in enumerate(eligibility):
                methods = entry["methods"]
                if not methods:
                    continue
                offset = (repeat + 2 + index) % len(methods)
                for method in methods[offset:] + methods[:offset]:
                    case = entry["case"]
                    receipt = invoke([out / "target/release/cut071", out / "source" / case["path"], method], out / "source", out / "commands" / f"r{repeat}_{case['id']}_{method}.json", timeout=120)
                    data = json.loads(receipt["stdout"])
                    if data["schema"] != 10 or data["method"] != method or [function["id"] for function in data["functions"]] != entry["function_ids"] or any(function["outcome"] != "Complete" for function in data["functions"]):
                        raise RuntimeError("frozen complete query changed; raw failure preserved")
                    log.write(json.dumps(dict(case=case["id"], method=method, repeat=repeat, measured=repeat>=0, wall_ns=receipt["wall_ns"], data=data), sort_keys=True)+"\n")
                    log.flush()
    summarize(out)
    write(out / "sample-finish.json", dict(window=args.window, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), raw_sha256=digest(raw), summary_sha256=digest(out / "summary.json"), inventory=inventory()))
    print(json.dumps(dict(out=str(out), raw_sha256=digest(raw))))


def summarize(out):
    records = [json.loads(line) for line in (out / "samples.jsonl").read_text().splitlines()]
    groups = {}
    def add(key, value):
        if type(value) in (int, float):
            groups.setdefault(key, []).append(value)
    for record in records:
        if not record["measured"]:
            continue
        common = (record["case"], record["method"])
        for key, value in record["data"].items():
            if key.endswith("_ns") or key == "process_peak_rss_kib":
                add((*common, "module", key), value)
        add((*common, "process", "wall_ns"), record["wall_ns"])
        for function in record["data"]["functions"]:
            for metric, value in function.items():
                if metric.endswith("_ns") or metric in ("candidate_work", "oracle_work", "bits", "physical_bits", "rows", "allocated_bdd_nodes", "created_bdd_nodes", "gc_work", "max_point_reachable_bdd_nodes", "materialized_bytes"):
                    add((*common, f"f{function['id']}:{function['name']}", metric), value)
    stats, ratios = [], []
    rng = random.Random(71)
    for key, values in sorted(groups.items()):
        if len(values) != 7:
            raise RuntimeError(f"incomplete sample group {key}")
        stats.append(dict(case=key[0], method=key[1], scope=key[2], metric=key[3], count=7, median=statistics.median(values), minimum=min(values), maximum=max(values)))
        if not (key[3].endswith("_ns") or key[3] == "process_peak_rss_kib"):
            continue
        controls = {
            "location:fused:owner:gc": ["dense:fused:owner:gc"],
            "local:fused:owner:gc": ["dense:fused:owner:gc", "location:fused:owner:gc"],
            "location:direct:owner:gc": ["dense:fused:owner:gc", "location:fused:owner:gc"],
            "local:direct:owner:gc": ["dense:fused:owner:gc", "location:direct:owner:gc", "local:fused:owner:gc"],
        }.get(key[1], [])
        for control in controls:
            other = groups.get((key[0], control, key[2], key[3]))
            if other is None or statistics.median(other) == 0:
                continue
            boots = []
            for _ in range(1000):
                indices = [rng.randrange(7) for _ in range(7)]
                denominator = statistics.median([other[i] for i in indices])
                if denominator:
                    boots.append(statistics.median([values[i] for i in indices])/denominator)
            boots.sort()
            ratios.append(dict(case=key[0], method=key[1], control=control, scope=key[2], metric=key[3], ratio_of_medians=statistics.median(values)/statistics.median(other), paired_bootstrap_95=[boots[int(len(boots)*.025)], boots[min(len(boots)-1, int(len(boots)*.975))]]))
    deterministic_work = {str(key): values[0] for key, values in groups.items() if key[3] in ("candidate_work", "oracle_work") and len(set(values)) == 1}
    if any(len(set(values)) != 1 for key, values in groups.items() if key[3] in ("candidate_work", "oracle_work")):
        raise RuntimeError("abstract work differs across fresh processes")
    write(out / "summary.json", dict(measured=sum(record["measured"] for record in records), warmups=sum(not record["measured"] for record in records), statistics=stats, ratios=ratios, deterministic_work=deterministic_work, bootstrap="1000 deterministic paired resamples seed71; exploratory small-sample uncertainty; not a product claim"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    prep.add_argument("--repo", required=True); prep.add_argument("--candidate", required=True); prep.add_argument("--out", required=True)
    timing = commands.add_parser("sample")
    timing.add_argument("--out", required=True); timing.add_argument("--window", required=True)
    for name in ("verify", "summarize"):
        commands.add_parser(name).add_argument("--out", required=True)
    args = parser.parse_args()
    if args.command == "prepare": prepare(args)
    elif args.command == "sample": sample(args)
    elif args.command == "verify": print(json.dumps(verify(Path(args.out).resolve())))
    else: summarize(Path(args.out).resolve())


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Replay the 076 relation algorithms on current frontend output and new distributions.

Preparation is correctness-only. Sampling requires an explicit coordinated window.
This measures a shadow relation query, never replaces production Analyze/Verify.
"""
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

METHODS = ["dense:direct:owner:gc", "local:direct:owner:gc", "local:intern:owner:gc"]
BDD_METHODS = ["dense:fused:owner:gc", "local:fused:owner:gc", "dense:cut:owner:gc"]
BDD_CASES = {"resources-sequential-1", "resources-sequential-4", "resources-sequential-8", "resources-diamond-1", "resources-diamond-2", "resources-diamond-3", "resources-diamond-4", "bundles", "loop", "guarded-desc", "large-128"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def resources(count, branch=False):
    flags = ",".join(f"b{i}:bool" for i in range(count)) if branch else ""
    text = "resource Token { value:i32 }\nfn consume(give token:Token) {}\n"
    text += f"fn work({flags})->i32 {{\n"
    for i in range(count):
        text += f" let r{i}=Token{{value:{i % 16}}}\n"
    for i in range(count):
        text += (f" if b{i} {{ consume(give r{i}) }}\n" if branch else f" consume(give r{i})\n")
    args = ",".join("true" if i % 2 else "false" for i in range(count)) if branch else ""
    return text + f" return 0\n}}\nfn main()->i32 {{ return work({args}) }}\n"


def invoke(binary, source, method, destination):
    command = [str(binary), str(source), method]
    start = time.perf_counter_ns()
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=90)
        row = {"command": command, "exit": result.returncode,
               "stdout": result.stdout, "stderr": result.stderr}
    except subprocess.TimeoutExpired as error:
        row = {"command": command, "exit": None, "outcome": "Timeout",
               "stdout": (error.stdout or b"").decode(), "stderr": (error.stderr or b"").decode()}
    row["wall_ns"] = time.perf_counter_ns() - start
    if row["exit"] == 0:
        row["result"] = json.loads(row["stdout"])
        row.pop("stdout")
    write(destination, row)
    return row


def summarize(row):
    if row["exit"] != 0:
        return {"complete": False, "outcome": row.get("outcome", "FrontendError")}
    result = row["result"]
    if "functions" not in result:
        return {"complete": result["outcome"] == "Accepted", "outcome": result["outcome"]}
    counts = dict(collections.Counter(f["outcome"] for f in result["functions"]))
    unexpected = set(counts) - {"Complete", "Unsupported", "UnsupportedMultiplicity",
                                "ResourceLimit", "OracleResourceLimit", "OracleUnsupported"}
    if unexpected:
        raise RuntimeError(f"query mismatch or internal error: {counts}")
    return {"complete": bool(result["functions"]) and set(counts) == {"Complete"},
            "outcomes": counts, "function_count": len(result["functions"]),
            "candidate_total_ns": result["candidate_total_ns"],
            "fingerprints": {f["id"]: f["canonical_fingerprint"] for f in result["functions"]
                             if f["outcome"] == "Complete"},
            "bits": sum(f.get("bits", 0) for f in result["functions"]),
            "physical_bits": sum(f.get("physical_bits", 0) for f in result["functions"]),
            "materialized_bytes": sum(f.get("materialized_bytes", 0) for f in result["functions"])}


def prepare(args):
    if args.out.exists():
        raise ValueError("Use a fresh output directory; failures must be retained")
    args.out.mkdir(parents=True)
    cases = []
    for count in [1, 4, 8, 16, 24, 32, 64]:
        name = f"resources-sequential-{count}"
        path = args.out / "sources" / name / "main.zil"
        path.parent.mkdir(parents=True)
        path.write_text(resources(count))
        cases.append((name, path, "generated real resource source"))
    for count in [1, 2, 3, 4, 5, 6, 8]:
        name = f"resources-diamond-{count}"
        path = args.out / "sources" / name / "main.zil"
        path.parent.mkdir(parents=True)
        path.write_text(resources(count, True))
        cases.append((name, path, "generated correlated cleanup source"))
    for name, path in [("bundles", "tests/perf/impl071/bundles.zil"),
                       ("loop", "tests/perf/impl059/loop.zil"),
                       ("guarded-desc", "tests/perf/impl059/guarded_desc.zil")]:
        cases.append((name, args.repo / path, "existing checked-in real source"))
    for name in ["large-128", "large-512", "large-2048", "large-4096", "large-8192",
                 "large-skew-8", "even-64x64", "body-16x16", "types-32x16"]:
        cases.append((name, args.stage5 / f"source-{name}/main.zil", "unchanged stage5 source"))
    manifest = {"binary": str(args.binary), "binary_sha256": digest(args.binary),
                "current_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.repo, text=True).strip(),
                "methods": METHODS, "bdd_methods": BDD_METHODS, "bdd_cases": sorted(BDD_CASES), "cases": [], "boundary": "Current TypedModule + adapted historical query. Full-output shadow research, not compiler implementation or CLI speed."}
    write(args.out / "manifest.json", manifest)
    for name, source, kind in cases:
        entry = {"name": name, "source": str(source), "kind": kind,
                 "sources": {str(p): digest(p) for p in sorted(source.parent.glob("*.zil"))},
                 "results": {}}
        production = invoke(args.binary, source, "production", args.out / "prepare" / name / "production.json")
        entry["production"] = summarize(production)
        fingerprints = {}
        for method in METHODS + (BDD_METHODS if name in BDD_CASES else []):
            row = invoke(args.binary, source, method, args.out / "prepare" / name / f"{method}.json")
            result = summarize(row)
            for function, fp in result.get("fingerprints", {}).items():
                if function in fingerprints and fingerprints[function] != fp:
                    raise RuntimeError(f"canonical cross-representation mismatch {name}/{function}")
                fingerprints[function] = fp
            entry["results"][method] = result
        entry["eligible_methods"] = [m for m, r in entry["results"].items() if r["complete"]]
        entry["timing_eligible"] = entry["production"]["complete"] and METHODS[0] in entry["eligible_methods"] and len(entry["eligible_methods"]) > 1
        manifest["cases"].append(entry)
        write(args.out / "manifest.json", manifest)
        print(name, entry["timing_eligible"], [r.get("outcomes", r.get("outcome")) for r in entry["results"].values()], flush=True)


def sample(args):
    if not args.window:
        raise ValueError("Explicit root-coordinated exclusive window required")
    manifest = json.loads((args.out / "manifest.json").read_text())
    assert digest(args.binary) == manifest["binary_sha256"]
    sample_path = args.out / "samples.jsonl"
    if sample_path.exists():
        raise ValueError("Preserve previous samples")
    write(args.out / "sample-window.json", {"window": args.window, "pid": os.getpid(), "blocks": 6,
                                          "scope": "Shadow query timing only; complete extraction/solve/oracle comparison/materialization included"})
    rows = []
    with sample_path.open("w") as stream:
        for entry in manifest["cases"]:
            if not entry["timing_eligible"]:
                continue
            for name, sha in entry["sources"].items():
                assert digest(Path(name)) == sha
            for block in range(6):
                order = entry["eligible_methods"] if block % 2 == 0 else list(reversed(entry["eligible_methods"]))
                for method in order:
                    row = invoke(args.binary, Path(entry["source"]), method,
                                 args.out / "sample-raw" / entry["name"] / str(block) / f"{method}.json")
                    summary = summarize(row)
                    assert summary["complete"]
                    assert summary["fingerprints"] == {int(k): v for k, v in entry["results"][method]["fingerprints"].items()}
                    data = {"case": entry["name"], "block": block, "method": method,
                            "wall_ns": row["wall_ns"], "candidate_ns": summary["candidate_total_ns"],
                            "rss_kib": row["result"]["process_peak_rss_kib"]}
                    rows.append(data)
                    stream.write(json.dumps(data) + "\n")
                    stream.flush()
    result = []
    for name in dict.fromkeys(r["case"] for r in rows):
        group = [r for r in rows if r["case"] == name]
        result.append({"case": name, "methods": {
            method: {metric: statistics.median(r[metric] for r in group if r["method"] == method)
                     for metric in ["wall_ns", "candidate_ns", "rss_kib"]} for method in dict.fromkeys(r["method"] for r in group)}})
    write(args.out / "summary.json", result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "sample"])
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--stage5", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--window")
    args = parser.parse_args()
    (prepare if args.mode == "prepare" else sample)(args)

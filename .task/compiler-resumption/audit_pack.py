#!/usr/bin/env python3
"""Recompute the fixed compiler experiment and preserve its raw evidence.

This tool never runs a compiler, changes an input, grants acceptance, or removes
original files. A completed audit may contain failed performance gates.
"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone
import hashlib
import io
import itertools
import json
import math
from pathlib import Path
import statistics
import tarfile

CASES = {"ring768": .1125, "shared1024": .1125, "diamond768": .1125,
         "trait32": .125, "trait128": .125, "owned8": .15,
         "owned32": .15, "shared4096": .1125}
MULTI = ("dispatch8", "pipeline8", "dispatch16", "pipeline16")
REQUESTS = ("cold", "warm", "private", "edit-warm", "public", "restore")
FREQUENCIES = {"edit": dict(zip(REQUESTS, (.1, .15, .45, .1, .1, .1))),
               "clean": dict(zip(REQUESTS, (.7, .05, .1, .05, .1, 0)))}
MODES = {"interactive": {"check": .3, "O0": .5, "O2": .2},
         "release": {"check": .1, "O0": .2, "O2": .7}}


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for part in iter(lambda: f.read(1024 * 1024), b""):
            h.update(part)
    return h.hexdigest()


def read_json(path):
    return json.loads(path.read_text())


def read_rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def write_new(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as f:
        json.dump(value, f, indent=2, ensure_ascii=False)
        f.write("\n")


def coverage(actual, expected, errors, label):
    counts = Counter(actual)
    missing, extra = expected - counts.keys(), counts.keys() - expected
    duplicates = sum(n - 1 for n in counts.values() if n > 1)
    if missing or extra or duplicates:
        errors.append({"lane": label, "missing": len(missing), "extra": len(extra),
                       "duplicates": duplicates, "missing_examples": sorted(missing)[:8]})
    return not (missing or extra or duplicates)


def finite_positive(value):
    return isinstance(value, (int, float)) and math.isfinite(value) and value > 0


def check_manifest(folder, errors, modes, rounds):
    path = folder / "manifest.json"
    manifest = read_json(path)
    if manifest.get("rounds") != rounds:
        errors.append({"lane": folder.name, "rounds_disagree_with_fixed_protocol": manifest.get("rounds")})
    if folder.name == "primary":
        observed_weights = {c["id"]: c["weight"] for c in manifest.get("cases", [])}
        if observed_weights != CASES or manifest.get("request_weights") != FREQUENCIES or manifest.get("mode_weights") != MODES:
            errors.append({"lane": folder.name, "frequency_contract_changed": True})
    elif manifest.get("modes") != list(modes):
        errors.append({"lane": folder.name, "mode_contract_changed": manifest.get("modes")})
    checked, failed = 0, []
    for name, expected in manifest.get("source_hashes", {}).items():
        p = (folder / name).resolve()
        if not p.is_relative_to(folder.resolve()) or not p.is_file() or digest(p) != expected:
            failed.append(name)
        checked += 1
    if not checked or failed:
        errors.append({"lane": folder.name, "source_hashes_checked": checked,
                       "source_hash_failures": failed})
    return {"source_hashes_checked": checked, "source_hash_failures": failed, "observed_tool_hashes": manifest.get("tools"),
            "historical_manifest_baseline_label": manifest.get("baseline")}


def compile_audit(root, lane, kind, rounds, errors):
    start = len(errors)
    folder = root / lane
    rows = read_rows(folder / "rows.jsonl")
    cases = CASES if lane == "primary" else dict.fromkeys(MULTI, .25)
    modes = ("check", "O0", "O2") if lane == "primary" or kind == "normalized" else ("O0", "O2")
    expected = set()
    for case, mode, request, block in itertools.product(cases, modes, REQUESTS, range(-1, rounds)):
        arms = ("base", "cand", "rust") if mode == "O2" and (lane == "multi" or case.startswith("owned")) else ("base", "cand")
        expected.update((case, mode, request, arm, block) for arm in arms)
    coverage([(r["case"], r["mode"], r["request"], r["arm"], r["block"]) for r in rows], expected, errors, lane)
    bad, raw_report_failures, grouped = [], [], defaultdict(list)
    for i, r in enumerate(rows):
        if (not r.get("valid") or r.get("exit") != 0 or r.get("timeout") or
                not finite_positive(r.get("wall_ms")) or r.get("warmup") != (r["block"] < 0)):
            bad.append(i)
        if r["arm"] != "rust":
            p = folder / "logs" / Path(r["log"]).name / "stdout"
            try:
                report = read_json(p)
                if report.get("status") != "Success" or report.get("verification_runs") != 1 or r.get("verification_runs") != 1:
                    raw_report_failures.append(i)
            except (OSError, ValueError):
                raw_report_failures.append(i)
        for item in r.get("native_checks", []):
            if item.get("actual") != item.get("expected"):
                bad.append(i)
        if r["block"] >= 0:
            grouped[(r["case"], r["mode"], r["request"], r["arm"])].append(r)
    if bad or raw_report_failures:
        errors.append({"lane": lane, "invalid_row_indices": sorted(set(bad)),
                       "raw_stdout_or_current_verify_failures": raw_report_failures})
    manifest = check_manifest(folder, errors, modes, rounds)
    complete = len(errors) == start
    profiles, worst = [], []
    if complete:
        mode_profiles = MODES if lane == "primary" else {"O2-only": {"O2": 1}}
        for mode_name, mw in mode_profiles.items():
            for request_name, rw in FREQUENCIES.items():
                means = {}
                for metric in ("wall_ms", "cpu_ms"):
                    means[metric] = {arm: sum(cw * mweight * rw[request] * statistics.mean(
                        r[metric] for r in grouped[(case, mode, request, arm)])
                        for case, cw in cases.items() for mode, mweight in mw.items()
                        for request in REQUESTS) for arm in ("base", "cand")}
                wall = means["wall_ms"]
                profiles.append({"profile": mode_name + "/" + request_name,
                                 "means": means, "reduction": 1 - wall["cand"] / wall["base"],
                                 "saved_ms_per_request": wall["base"] - wall["cand"]})
        for case, mode, request in itertools.product(cases, modes, REQUESTS):
            means = {arm: statistics.mean(r["wall_ms"] for r in grouped[(case, mode, request, arm)]) for arm in ("base", "cand")}
            worst.append({"case": case, "mode": mode, "request": request, "mean_ms": means,
                          "ratio": means["cand"] / means["base"], "delta_ms": means["cand"] - means["base"]})
    return {"complete": complete, "compile_requests": len(rows), "expected_requests": len(expected),
            "formal_requests": sum(r["block"] >= 0 for r in rows), "actual_current_verify_reports":
            sum(r["arm"] != "rust" for r in rows) - len(raw_report_failures),
            "native_output_checks": sum(len(r.get("native_checks", [])) for r in rows),
            "modes": modes, "manifest": manifest, "profiles": profiles,
            "worst_compile_ratios": sorted(worst, key=lambda x: x["ratio"], reverse=True)[:5],
            "separate_from_original_aggregate": lane == "multi"}


def runtime_rows(root, lane):
    folder = root / lane
    if lane in ("primary", "multi"):
        result = read_rows(folder / "runtime.jsonl")
        for r in result:
            p = folder / "runtime-logs" / Path(r["log"]).name
            r["actual_output"] = (p / "stdout").read_bytes().hex()
        return result
    result = []
    for row in read_json(folder / "rows.json"):
        parts = row["label"].split("/")
        if parts[-1] == "build":
            continue
        if lane == "quality":
            n, snapshot, depth, repetition, arm = parts
            key = {"case": "owned" + n, "snapshot": snapshot, "depth": int(depth.removeprefix("depth"))}
        else:
            variant, n, depth, repetition, arm = parts
            key = {"case": variant + "/" + n, "snapshot": "base", "depth": int(depth)}
        result.append(dict(key, arm=arm, repetition=int(repetition), wall_ms=row["ms"],
                           exit=row["exit"], valid=row["exit"] == 0, actual_output=row["stdout"]))
    return result


def elf_identity(root, lane, key, archived):
    case, snapshot, _ = key
    prefix = Path(lane, "quality", case, snapshot) if lane in ("primary", "multi") else (
        Path(lane, case, snapshot) if lane == "quality" else Path(lane, case))
    hashes, sources = {}, {}
    for arm in ("base", "cand"):
        rel = prefix / arm / "app"
        path = root / rel
        if path.is_file():
            with path.open("rb") as f:
                if f.read(4) != b"\x7fELF":
                    continue
            hashes[arm], sources[arm] = digest(path), "actual complete ELF"
        elif str(rel) in archived:
            hashes[arm], sources[arm] = archived[str(rel)], "preserved complete-ELF digest"
    return {"complete_elf_identical": len(hashes) == 2 and hashes["base"] == hashes["cand"],
            "sha256": hashes, "identity_sources": sources}


def runtime_audit(root, lane, errors, archived):
    start = len(errors)
    rows = runtime_rows(root, lane)
    if lane == "primary":
        cells = [(c, s, None) for c in CASES for s in ("base", "private")]
        reps = 5
    elif lane == "multi":
        cells = list(itertools.product(MULTI, ("base", "private"), (0, 11)))
        reps = 7
    elif lane == "quality":
        cells = list(itertools.product(("owned8", "owned16", "owned24", "owned32", "owned64"), ("base", "private"), (0, 5, 11)))
        reps = 9
    else:
        cells = [("unentered/" + str(n), "base", 0) for n in (8, 16, 32, 64)]
        cells += [(v + "/" + str(n), "base", d) for v in ("one_hot", "unused_definitions") for n in (16, 32, 64) for d in (1, 6)]
        reps = 9
    expected = set()
    for key in cells:
        arms = ("base", "cand") if lane == "primary" and not key[0].startswith("owned") else ("base", "cand", "rust")
        expected.update(key + (arm, rep) for arm in arms for rep in range(-1, reps))
    keys = [(r["case"], r["snapshot"], r.get("depth"), r["arm"], r["repetition"]) for r in rows]
    coverage(keys, expected, errors, lane + "/runtime")
    groups, outputs, bad = defaultdict(lambda: defaultdict(list)), defaultdict(set), []
    for i, r in enumerate(rows):
        key = (r["case"], r["snapshot"], r.get("depth"))
        if not r.get("valid") or r.get("exit") != 0 or not finite_positive(r.get("wall_ms")):
            bad.append(i)
        outputs[key].add(r["actual_output"])
        if r["repetition"] >= 0:
            groups[key][r["arm"]].append(r["wall_ms"])
    if bad or any(len(v) != 1 for v in outputs.values()):
        errors.append({"lane": lane + "/runtime", "invalid_rows": bad,
                       "output_mismatch_cells": [k for k, v in outputs.items() if len(v) != 1]})
    results = []
    for key in cells:
        values = groups[key]
        arms = ("base", "cand") if lane == "primary" and not key[0].startswith("owned") else ("base", "cand", "rust")
        if any(len(values[a]) != reps for a in arms):
            continue
        med = {a: statistics.median(values[a]) for a in arms}
        ratio = med["cand"] / med["base"]
        base90 = med["rust"] / med["base"] >= .9 if "rust" in med else None
        cand90 = med["rust"] / med["cand"] >= .9 if "rust" in med else None
        results.append({"case": key, "median_ms": med, "candidate_latency_ratio": ratio,
                        "new_over_10pct": ratio > 1.1, "baseline_rust90_pass": base90,
                        "candidate_rust90_pass": cand90, "new_rust90_crossing": base90 is True and cand90 is False,
                        "preexisting_rust90_failure": base90 is False,
                        **elf_identity(root, lane, key, archived)})
    flagged = [c for c in results if c["new_over_10pct"] or c["new_rust90_crossing"] or c["candidate_rust90_pass"] is False]
    return {"complete": len(errors) == start, "runtime_requests": len(rows), "expected_requests": len(expected),
            "formal_samples_per_arm": reps, "cells": len(results),
            "new_over_10pct_count": sum(c["new_over_10pct"] for c in results),
            "new_rust90_crossing_count": sum(c["new_rust90_crossing"] for c in results),
            "preexisting_rust90_failure_count": sum(c["preexisting_rust90_failure"] for c in results),
            "candidate_rust90_failure_count": sum(c["candidate_rust90_pass"] is False for c in results),
            "observed_all_runtime_constraints_pass": len(errors) == start and len(results) == len(cells) and not flagged,
            "flagged_cells": flagged, "worst_latency_cells": sorted(results, key=lambda c: c["candidate_latency_ratio"], reverse=True)[:2]}


def control_audit(root, lane, errors):
    start = len(errors)
    data = read_json(root / lane / "summary.json")
    multi = lane == "multi-controls"
    names = list(itertools.product(MULTI, ("llvm-api", "clang"), ("O0", "O2"), ("base", "cand"))) if multi else list(itertools.product(
        ("owned8", "owned32", "owned64"), ("llvm-api", "clang"), ("O0", "O2"), ("1", "8"), ("base", "cand")))
    builds = data.get("build_reports", [])
    name_of = (lambda x: (x["case"], x["backend"], x["opt"], x["arm"])) if multi else (lambda x: tuple(x["name"].split("/")))
    coverage([name_of(b) for b in builds], set(names), errors, lane + "/builds")
    failed_builds = [name_of(b) for b in builds if b["report"].get("status") != "Success" or b["report"].get("verification_runs") != 1]
    observations = data.get("observations_detail", [])
    fails = (0, 1, 4, 16, 64) if multi else (0, 1, 2, 4, 8, 16, 32, 64, 128)
    inputs = tuple(bytes((s, 3, 0, 2)).hex() for s in (0, 3, 7, 15)) if multi else ("49030001", "1d010002")
    expected = {name + (fail, inp) for name in names for fail in fails for inp in inputs}
    obs_key = lambda x: name_of(x) + (x["fail"] if multi else x["fail_at"], x["input"])
    coverage([obs_key(o) for o in observations], expected, errors, lane + "/observations")
    pairs, bad = defaultdict(dict), []
    for o in observations:
        name = name_of(o)
        allocation = o["allocation"]
        if allocation.get("live") != 0 or allocation.get("allocated") != allocation.get("freed"):
            bad.append(obs_key(o))
        pair = name[:-1] + (o["fail"] if multi else o["fail_at"], o["input"])
        pairs[pair][name[-1]] = (o["output"], allocation)
    unequal = [k for k, v in pairs.items() if v.get("base") != v.get("cand")]
    if failed_builds or bad or unequal:
        errors.append({"lane": lane, "failed_builds": failed_builds, "unreleased_allocations": bad, "unequal_pairs": unequal})
    return {"complete": len(errors) == start, "builds": len(builds), "observations": len(observations),
            "paired_observations": len(pairs), "recorded_status": data.get("status", data.get("complete"))}


def audit(args):
    root = args.root.resolve()
    errors, compile_data, runtime_data, controls, flags = [], {}, {}, {}, {}
    archived = {}
    if args.elf_index:
        raw = read_json(args.elf_index)
        items = raw if isinstance(raw, list) else raw.get("excluded", [])
        archived = {v["path"].removeprefix("evidence/"): v["sha256"] for v in items if v.get("sha256")}
    for lane in ("primary", "multi", "quality", "neighbors", "controls") + (("multi-controls",) if args.kind == "resource" else ()):
        try:
            original = read_json(root / lane / "summary.json")
            flags[lane] = {k: original[k] for k in ("status", "complete", "runtime_gate_pass", "no_new_runtime_regression", "rust90_all") if k in original}
            if lane in ("primary", "multi"):
                compile_data[lane] = compile_audit(root, lane, args.kind, args.rounds, errors)
            if lane in ("primary", "multi", "quality", "neighbors"):
                runtime_data[lane] = runtime_audit(root, lane, errors, archived)
            if "controls" in lane:
                controls[lane] = control_audit(root, lane, errors)
        except (OSError, ValueError, KeyError, TypeError, IndexError) as exc:
            errors.append({"lane": lane, "error": str(exc)})
    result = {"schema": 1, "observed_at": datetime.now(timezone(timedelta(hours=8))).isoformat(),
              "actual_baseline": args.baseline, "actual_candidate": args.candidate, "kind": args.kind,
              "complete": not errors, "errors": errors, "compile": compile_data, "runtime": runtime_data,
              "controls": controls, "original_summary_flags_unchanged": flags,
              "frequency_contract": {"cases": CASES, "requests": FREQUENCIES, "modes": MODES, "multi": "four equal cases, separate O2-only distribution"},
              "decision": "Not made. An identical complete ELF is an explanation, never a rewritten false gate. Existing Rust90 deficits remain open."}
    write_new(args.output, result)
    print(json.dumps({"complete": result["complete"], "errors": len(errors), "output": str(args.output),
                      "new_over_10pct": sum(x["new_over_10pct_count"] for x in runtime_data.values()),
                      "new_rust90_crossings": sum(x["new_rust90_crossing_count"] for x in runtime_data.values())}))
    return 0 if not errors else 2


def pack(args):
    output = args.output.resolve()
    if output.exists() or Path(str(output) + ".partial").exists():
        raise ValueError("Archive already exists; use a new name to preserve the earlier outcome")
    sources = [("evidence", args.root.resolve())]
    for item in args.add:
        label, path = item.split("=", 1)
        if not label or label.startswith("/") or ".." in Path(label).parts:
            raise ValueError("Unsafe extra label")
        sources.append((label, Path(path).resolve()))
    included, excluded, selected, names = [], [], [], set()
    cache_parts = {"target", "incremental", "semantic", "objects", "__pycache__", ".git"}
    for label, source in sources:
        if not source.exists() or (source.is_dir() and output.is_relative_to(source)):
            raise ValueError("Inputs must exist, and archive output must be outside every input directory")
        paths = source.rglob("*") if source.is_dir() else (source,)
        for path in sorted(paths):
            rel = path.relative_to(source) if source.is_dir() else Path(source.name)
            name = str(Path(label) / rel)
            if path.is_symlink():
                excluded.append({"path": name, "reason": "symlink not followed"})
                continue
            if path.is_dir():
                continue
            if name in names:
                raise ValueError("Duplicate archive member: " + name)
            names.add(name)
            if not path.is_file():
                continue
            st = path.stat()
            with path.open("rb") as f:
                elf = f.read(4) == b"\x7fELF"
            cached = bool(cache_parts.intersection(rel.parts))
            record = {"path": name, "bytes": st.st_size}
            if elf or cached:
                record["reason"] = "complete ELF" if elf else "reproducible cache/target"
                if elf:
                    record["sha256"] = digest(path)
                excluded.append(record)
                continue
            record["sha256"] = digest(path)
            included.append(record)
            selected.append((path, name, st.st_size, st.st_mtime_ns))
    manifest = {"included": included, "excluded": excluded, "original_files_deleted": False,
                "excluded_elf_count": sum(e["reason"] == "complete ELF" for e in excluded)}
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = Path(str(output) + ".partial")
    with tarfile.open(partial, "w:gz", compresslevel=6) as tar:
        for path, name, size, mtime in selected:
            st = path.stat()
            if (st.st_size, st.st_mtime_ns) != (size, mtime):
                raise ValueError("Input changed during packing: " + str(path))
            tar.add(path, arcname=name, recursive=False)
        data = (json.dumps(manifest, indent=2) + "\n").encode()
        info = tarfile.TarInfo("evidence-manifest.json")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    for path, _, size, mtime in selected:
        st = path.stat()
        if (st.st_size, st.st_mtime_ns) != (size, mtime):
            raise ValueError("Input changed during packing: " + str(path))
    if partial.stat().st_size >= 48 * 1024 * 1024:
        raise ValueError("Archive exceeds the strict 48 MiB cap; partial archive retained, originals unchanged")
    partial.rename(output)
    receipt = {"archive": str(output), "bytes": output.stat().st_size, "sha256": digest(output),
               "included_files": len(included), "excluded_files": len(excluded),
               "excluded_elf_count": manifest["excluded_elf_count"], "original_files_deleted": False}
    write_new(Path(str(output) + ".receipt.json"), receipt)
    print(json.dumps(receipt))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("audit")
    p.add_argument("root", type=Path)
    p.add_argument("--kind", choices=("resource", "normalized"), required=True)
    p.add_argument("--baseline", required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--rounds", type=int, default=5)
    p.add_argument("--elf-index", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p = sub.add_parser("pack")
    p.add_argument("root", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--add", action="append", default=[], metavar="LABEL=PATH",
                   help="Explicit extra scripts or toolchain metadata; never pass an entire toolchain installation")
    args = parser.parse_args()
    return audit(args) if args.command == "audit" else pack(args)


if __name__ == "__main__":
    raise SystemExit(main())

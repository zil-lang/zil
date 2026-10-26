#!/usr/bin/env python3
"""Derive a compact receipt from frozen raw076 records; never rewrite those records."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import statistics


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    root = Path(args.out).resolve()
    receipt = Path(args.receipt)
    if receipt.exists():
        raise RuntimeError("new derived receipt required")
    identity = json.loads((root / "identity.json").read_text())
    summary = json.loads((root / "summary.json").read_text())
    samples = [json.loads(line) for line in (root / "samples.jsonl").read_text().splitlines()]
    outcomes = Counter()
    censored = []
    pairs = 0
    queries = list((root / "freeze/graph").glob("*/*/data.json"))
    for file in queries:
        data = json.loads(file.read_text())
        for function in data["functions"]:
            outcomes[function["outcome"]] += 1
            if function["outcome"] in ("Complete", "UnsupportedMultiplicity"):
                stem = file.parent / "relations" / f"f{function['id']}"
                candidate = Path(str(stem) + ".json").read_bytes()
                oracle = Path(str(stem) + ".oracle.json").read_bytes()
                if candidate != oracle:
                    raise RuntimeError("full canonical pair mismatch")
                pairs += 1
            else:
                censored.append(dict(case=file.parents[1].name, method=data["method"], function=function["name"], outcome=function["outcome"], error=function.get("error"), oracle_error=function.get("oracle_error"), oracle_outcome=function.get("oracle_outcome")))
    sensitivity = []
    for case in sorted({record["case"] for record in samples}):
        methods = {}
        for method in ("dense:fused:owner:gc", "local:direct:owner:gc"):
            rows = [record["data"] for record in samples if record["measured"] and record["case"] == case and record["method"] == method]
            census = [sum(function["census_ns"] for function in data["functions"]) for data in rows]
            adjusted = [data["candidate_total_ns"] - cost for data, cost in zip(rows, census)]
            methods[method] = dict(census_median_ns=statistics.median(census), adjusted_candidate_median_ns=statistics.median(adjusted))
        sensitivity.append(dict(case=case, methods=methods, adjusted_ratio=methods["local:direct:owner:gc"]["adjusted_candidate_median_ns"] / methods["dense:fused:owner:gc"]["adjusted_candidate_median_ns"]))
    result = dict(
        source_baseline=identity["source_baseline"], research_baseline=identity["research_baseline"], previous_research_core=identity["previous_research_core"], research_commit=identity["research_commit"], schema=10,
        input_families=len({p.parents[1].name for p in queries}), functional_processes=len(queries), function_queries=sum(outcomes.values()), outcomes=dict(outcomes), canonical_pairs=pairs,
        warmup_processes=summary["warmups"], measured_processes=summary["measured"], deterministic_work_arrays=len(summary["deterministic_work"]),
        medians=[stat for stat in summary["statistics"] if (stat["scope"] == "module" and stat["metric"] in ("candidate_total_ns", "process_peak_rss_kib")) or (stat["scope"] == "process" and stat["metric"] == "wall_ns")],
        ratios=[ratio for ratio in summary["ratios"] if (ratio["scope"] == "module" and ratio["metric"] in ("candidate_total_ns", "process_peak_rss_kib")) or (ratio["scope"] == "process" and ratio["metric"] == "wall_ns")],
        census_sensitivity=sensitivity,
        metadata_notes=[
            "Optional Telemetry is disabled. The common report still traverses point BDD shape; this is charged and timed. Explicit variants have no state BDD shape traversal.",
            "Census sensitivity subtracts recorded phase durations arithmetically; it is not a rerun without census and does not model secondary cache effects.",
            "identity.formal_samples=NotRun is the immutable preparation snapshot; actual formal completion is recorded in sample-finish.json.",
            "VmHWM is whole-process peak RSS including frontend/oracle/candidate, not candidate-only memory.",
            "BDD and direct work models differ. Same numeric work caps do not imply equal CPU cost.",
        ],
        censored=censored,
        raw_hashes={name:digest(root / name) for name in ("identity.json", "source-manifest.json", "freeze-manifest.json", "samples.jsonl", "summary.json", "sample-start.json", "sample-finish.json")},
        research_verdict="Analyze: compare dense-direct, dense-intern and local-intern with complete output; explicit rows remain exponential",
        production_admission="NoGo: frozen research is not the production safety kernel, full language, or a Rust comparison",
    )
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({key:result[key] for key in ("functional_processes", "function_queries", "outcomes", "canonical_pairs", "warmup_processes", "measured_processes")}))


if __name__ == "__main__":
    main()

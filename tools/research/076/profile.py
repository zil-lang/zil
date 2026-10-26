#!/usr/bin/env python3
"""Derive full-cost and exact sharing receipts without modifying frozen measurements."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    root, destination = Path(args.out), Path(args.receipt)
    if destination.exists():
        raise RuntimeError("new derived receipt required")
    records = [json.loads(line) for line in (root / "samples.jsonl").read_text().splitlines()]
    groups = {}
    for record in records:
        if record["measured"]:
            groups.setdefault((record["case"], record["method"]), []).append(record)
    phases = ("extract_ns", "event_ns", "plan_ns", "solve_ns", "check_ns", "census_ns", "materialize_ns", "drop_ns")
    profiles = []
    for (case, method), rows in sorted(groups.items()):
        if len(rows) != 7:
            raise RuntimeError("incomplete group")
        total = [row["data"]["candidate_total_ns"] for row in rows]
        observations = [sum(f["check_ns"] + f["materialize_ns"] for f in row["data"]["functions"]) for row in rows]
        census = [sum(f["census_ns"] for f in row["data"]["functions"]) for row in rows]
        profiles.append(dict(case=case, method=method,
            module_medians={key:statistics.median(row["data"][key] for row in rows) for key in ("candidate_total_ns", "oracle_total_ns", "frontend_ns", "total_ns", "process_peak_rss_kib")},
            process_wall_median_ns=statistics.median(row["wall_ns"] for row in rows),
            phase_sum_medians={phase:statistics.median(sum(f[phase] for f in row["data"]["functions"]) for row in rows) for phase in phases},
            check_plus_materialize_share_median=statistics.median(o/t for o,t in zip(observations,total)),
            arithmetic_census_subtracted_median_ns=statistics.median(t-c for t,c in zip(total,census)),
            intern_expand_median_ns=statistics.median(sum(f.get("intern_stats", {}).get("expand_ns",0) for f in row["data"]["functions"]) for row in rows)))
    structural = []
    for path in sorted((root / "freeze/graph").glob("*/*/data.json")):
        data = json.loads(path.read_text())
        for function in data["functions"]:
            if "intern_stats" in function:
                stat = {key:value for key,value in function["intern_stats"].items() if key != "expand_ns"}
                structural.append(dict(case=path.parents[1].name, method=data["method"], function=function["name"], id=function["id"], outcome=function["outcome"], **stat))
    for record in records:
        for function in record["data"]["functions"]:
            if "intern_stats" not in function:
                continue
            expected = next(item for item in structural if item["case"]==record["case"] and item["method"]==record["method"] and item["id"]==function["id"])
            for key,value in function["intern_stats"].items():
                if key != "expand_ns" and expected[key] != value:
                    raise RuntimeError("intern structure differs across processes")
    result = dict(source_commit=json.loads((root / "identity.json").read_text())["research_commit"],
        samples_sha256=hashlib.sha256((root / "samples.jsonl").read_bytes()).hexdigest(),
        profiles=profiles, exact_intern_structure=structural,
        notes=["Phase medians are computed separately and need not sum to the candidate median.",
               "Census subtraction is arithmetic sensitivity, not a rerun or cache-effect measurement.",
               "Intern row payload excludes tables, metadata, IDs, temporary rows and final expansion; it is not peak memory.",
               "Full-process wall and VmHWM include frontend, independent oracle, candidate, startup and reporting."])
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(result, indent=2, sort_keys=True)+"\n")
    print(json.dumps(dict(profiles=len(profiles), intern_functions=len(structural), deterministic_intern_censuses=True)))


if __name__ == "__main__":
    main()

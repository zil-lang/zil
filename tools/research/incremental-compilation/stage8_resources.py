#!/usr/bin/env python3
"""Summarize wait4 resources from a validated Stage 8 window; no new executions."""
import argparse
import json
import math
import statistics
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=Path)
args = parser.parse_args()
rows = [json.loads(line) for line in (args.directory / "rows.jsonl").read_text().splitlines()]
analysis = json.loads((args.directory / "analysis.json").read_text())
assert len(rows) == analysis["requests"] == analysis["valid"]
assert all(row["valid"] for row in rows)
lookup = {(r["case"], r["contract"], r["request"], r["rep"], r["arm"]): r for r in rows}
assert len(lookup) == len(rows)
result = []
for cell in analysis["cells"]:
    item = {key: cell[key] for key in ("case", "contract", "request", "repetitions")}
    for metric, scale in (("cpu_seconds", 1), ("max_rss_kib", 1 / 1024)):
        values = {}
        for arm in ("baseline", "candidate"):
            values[arm] = [
                lookup[cell["case"], cell["contract"], cell["request"], rep, arm][metric] * scale
                for rep in range(cell["repetitions"])
            ]
        pairs = [new / old for new, old in zip(values["candidate"], values["baseline"])]
        item[metric] = {
            "unit": "seconds" if metric == "cpu_seconds" else "MiB",
            "median": {arm: statistics.median(v) for arm, v in values.items()},
            "candidate_baseline_geomean": math.exp(statistics.mean(map(math.log, pairs))),
            "paired_range": [min(pairs), max(pairs)],
        }
    result.append(item)
output = {
    "scope": "Linux wait4 CPU usage and process maximum RSS; RSS is not simultaneous tree-wide memory. Same paired identities as analysis.json; no extra samples.",
    "cells": result,
}
(args.directory / "resources.json").write_text(json.dumps(output, indent=2) + "\n")
for item in result:
    print(item["case"], item["contract"], item["request"], {
        metric: {"median": v["median"], "new/old": v["candidate_baseline_geomean"]}
        for metric, v in item.items() if metric in ("cpu_seconds", "max_rss_kib")
    })

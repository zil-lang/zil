#!/usr/bin/env python3
"""Six-block shadow-query process comparisons and separate compressed-capacity costs."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("--factor", type=Path, required=True)
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--controls", type=Path, required=True)
parser.add_argument("--out", type=Path, required=True)
parser.add_argument("--window", required=True)
args = parser.parse_args()
assert args.window and not args.out.exists()
args.out.mkdir(parents=True)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
rows = []
cases = [f"bool-{n}" for n in [1, 2, 4, 8]] + [f"sequence-{n}" for n in [1, 4, 8, 16]]
large = [f"bool-{n}" for n in [16, 32, 64, 128]]
identity = {"window": args.window, "factor_sha256": sha(args.factor),
            "baseline_sha256": sha(args.baseline), "runner_sha256": sha(Path(__file__)),
            "scope": "Actual frontend + shadow relation query process cost. Small comparisons all reconstruct identical complete canonical rows; factor emits additional compressed metadata. Large cases only report compressed-query costs, no ratio against failed full enumeration.",
            "cases": cases, "compressed_only": large, "blocks": 6}
(args.out / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
with (args.out / "rows.jsonl").open("w") as log:
    for case in cases + large:
        receipt = json.loads((args.controls / f"{case}.factor.command.json").read_text())
        source = Path(receipt["command"][1])
        assert sha(source) == receipt["source_sha256"]
        expected = json.loads((args.controls / f"{case}.factor.json").read_text())
        fingerprints = {f["function"]: f["canonical_fingerprint"] for f in expected["functions"] if f.get("check") == "ExactOracleRows"}
        for block in range(6):
            arms = ["baseline", "factor"] if block % 2 == 0 else ["factor", "baseline"]
            if case in large:
                arms = ["factor"]
            for arm in arms:
                command = [str(args.factor if arm == "factor" else args.baseline), str(source)]
                if arm == "baseline":
                    command.append("local:direct:owner:gc")
                start = time.perf_counter_ns()
                result = subprocess.run(command, capture_output=True, text=True, timeout=60)
                wall_ns = time.perf_counter_ns() - start
                destination = args.out / f"{case}-{block}-{arm}.json"
                assert result.returncode == 0, result.stderr
                data = json.loads(result.stdout)
                if arm == "baseline":
                    assert all(f["outcome"] == "Complete" for f in data["functions"])
                    actual = {f["id"]: f["canonical_fingerprint"] for f in data["functions"]}
                else:
                    assert all(f["outcome"] == "CompleteFactored" for f in data["functions"])
                    actual = {f["function"]: f["canonical_fingerprint"] for f in data["functions"] if f["check"] == "ExactOracleRows"}
                    if case in large:
                        work = next(f for f in data["functions"] if f["name"] == "work")
                        assert work["check"] == "ExpansionDeferred" and work["canonical_fingerprint"] is None
                assert actual == fingerprints
                destination.write_text(result.stdout)
                row = {"case": case, "block": block, "arm": arm, "command": command,
                       "wall_ns": wall_ns, "stdout_bytes": len(result.stdout.encode()),
                       "stderr": result.stderr, "output_sha256": sha(destination)}
                rows.append(row)
                log.write(json.dumps(row) + "\n")
                log.flush()
summary = []
for case in cases + large:
    group = [r for r in rows if r["case"] == case]
    fixed = {r["block"]: r for r in group if r["arm"] == "factor"}
    old = {r["block"]: r for r in group if r["arm"] == "baseline"}
    summary.append({"case": case, "factor_wall_ms": statistics.median(r["wall_ns"] for r in fixed.values()) / 1e6,
                    "factor_stdout_bytes": statistics.median(r["stdout_bytes"] for r in fixed.values()),
                    "baseline_wall_ms": statistics.median(r["wall_ns"] for r in old.values()) / 1e6 if old else None,
                    "paired_wall_ratio": statistics.median(fixed[b]["wall_ns"] / old[b]["wall_ns"] for b in old) if old else None})
(args.out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))

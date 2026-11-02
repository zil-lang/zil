#!/usr/bin/env python3
"""Identical zit_bench.mjs workers; Python orchestration for restricted Node spawnSync."""
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys
import time

source, graph_dir, output = map(Path, sys.argv[1:4])
blocks, edits = map(int, sys.argv[4:6])
assert blocks > 0 and blocks % 2 == 0 and edits >= 2
assert not output.exists()
worker = Path(__file__).with_suffix(".mjs")
adapter = worker.with_name("zit_adapter.mjs")
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
cases = ["large-128", "large-512", "large-2048", "large-8192", "large-skew-8", "even-64x64", "body-16x16"]
rows, graphs = [], []
raw = output.with_suffix(".jsonl")
assert not raw.exists()
with raw.open("w") as stream:
    for name in cases:
        graph_path = graph_dir / f"callgraph-{name}.json"
        graph = json.loads(graph_path.read_text())
        graphs.append({"case": name, "path": str(graph_path), "sha256": sha(graph_path), "provenance": graph["provenance"]})
        for block in range(blocks):
            for arm in (["original", "adapted"] if block % 2 == 0 else ["adapted", "original"]):
                command = ["node", "--expose-gc", str(worker), "--worker", str(source), str(graph_path), arm, str(edits)]
                start = time.perf_counter_ns()
                process = subprocess.run(command, capture_output=True, text=True, timeout=120)
                wall_ms = (time.perf_counter_ns() - start) / 1e6
                if process.returncode or not process.stdout.strip():
                    stream.write(json.dumps({"case": name, "block": block, "arm": arm, "command": command, "exit": process.returncode, "stdout": process.stdout, "stderr": process.stderr}) + "\n")
                    raise RuntimeError(f"worker failed: {name}/{block}/{arm}: {process.stderr}")
                row = {"case": name, "block": block, **json.loads(process.stdout), "workerWallMs": wall_ms, "command": command, "stderr": process.stderr}
                rows.append(row)
                stream.write(json.dumps(row) + "\n")
                stream.flush()
summary = []
for name in cases:
    group = [r for r in rows if r["case"] == name]
    old = {r["block"]: r for r in group if r["arm"] == "original"}
    new = {r["block"]: r for r in group if r["arm"] == "adapted"}
    result = {"case": name}
    for label, data in [("original", old), ("adapted", new)]:
        for metric in ["coldMs", "editMeanMs", "failureRecoveryMs", "workerWallMs"]:
            result[label + metric[0].upper() + metric[1:]] = statistics.median(r[metric] for r in data.values())
    result["pairedColdRatio"] = statistics.median(new[b]["coldMs"] / old[b]["coldMs"] for b in old)
    result["pairedEditRatio"] = statistics.median(new[b]["editMs"] / old[b]["editMs"] for b in old)
    summary.append(result)
result = {"scope": "Original/adapted synchronous numerical zit kernel on actual LLVM direct-call topology. Not semantic compilation or CLI speed. Each arm fresh Node process. Failure contracts differ; no failure-speed ratio.",
          "blocks": blocks, "edits": edits, "node": subprocess.check_output(["node", "--version"], text=True).strip(),
          "source_sha256": sha(source), "adapter_sha256": sha(adapter), "worker_sha256": sha(worker), "runner_sha256": sha(Path(__file__)),
          "graphs": graphs, "rows": rows, "summary": summary}
output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(summary, indent=2))

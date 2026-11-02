#!/usr/bin/env python3
"""Extract direct-call DAGs from preserved, actual pre-partition LLVM modules."""
import argparse
import collections
import hashlib
import json
import re
from pathlib import Path


def extract(path):
    raw = path.read_bytes()
    functions = {}
    current = None
    symbol = r'@("[^"]+"|[-\w.]+)\('
    for line in raw.decode().splitlines():
        if line.startswith("define "):
            current = re.search(symbol, line).group(1)
            functions[current] = set()
        elif current and line == "}":
            current = None
        elif current and re.search(r"\b(call|invoke)\b", line):
            target = re.search(symbol, line)
            if target:
                functions[current].add(target.group(1))
    deps = {name: targets & functions.keys() for name, targets in functions.items()}
    consumers = {name: [] for name in functions}
    counts = {name: len(targets) for name, targets in deps.items()}
    for name in functions:
        for dependency in deps[name]:
            consumers[dependency].append(name)
    queue = collections.deque(name for name in functions if not counts[name])
    order = []
    while queue:
        name = queue.popleft()
        order.append(name)
        for consumer in consumers[name]:
            counts[consumer] -= 1
            if not counts[consumer]:
                queue.append(consumer)
    if len(order) != len(functions):
        raise ValueError("Actual call graph contains recursion; requires SCC query semantics")
    indices = {name: index for index, name in enumerate(order)}
    return {
        "provenance": {
            "artifact": str(path.resolve()),
            "sha256": hashlib.sha256(raw).hexdigest(),
            "boundary": "Actual public pre-partition LLVM direct-call topology. Numeric replay is a scheduler control, not compiler semantic queries or CLI timing.",
        },
        "nodes": [
            {"name": name, "deps": sorted(indices[d] for d in deps[name])}
            for name in order
        ],
        "edit_node": next(i for i, n in enumerate(order) if n.strip('"').startswith("zil.fn.") and not deps[n]),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--production-commit", required=True)
    parser.add_argument("cases", nargs="+")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for case in args.cases:
        result = extract(args.root / f"{case}-O0-0-0-zil/program.ll")
        result["case"] = case
        result["provenance"]["production_commit"] = args.production_commit
        (args.out / f"callgraph-{case}.json").write_text(json.dumps(result, indent=2) + "\n")

#!/usr/bin/env python3
"""Finite graph-model checks, NOT compiler performance or language validation.

The candidate keeps the original complete-closure copy bound. The oracle uses
an independent Floyd-Warshall transitive closure, not the planner's traversal.
Only call graphs and source-group ownership are modelled; LLVM/linkage, exact
cache identity, source acceptance and native execution require Rust gates.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import random


def one_attempt(edges, groups, costs, count):
    owners = [g % max(1, count) for g in groups]
    members = {}
    for i, owner in enumerate(owners):
        members.setdefault(owner, []).append(i)
    if count < 2 or len(members) <= 1:
        return {"units": 1, "owners": owners, "imports": {}, "copied": 0, "fallback": "single"}
    copied, imports = 0, {}
    for owner, definitions in sorted(members.items()):
        pending = set(j for i in definitions for j in edges[i])
        seen, imported = set(), set()
        while pending:
            i = min(pending)
            pending.remove(i)
            if owners[i] == owner or i in seen:
                continue
            seen.add(i)
            copied += costs[i]
            if copied > sum(costs):
                return {"units": 1, "owners": owners, "imports": {}, "copied": 0, "fallback": "budget"}
            imported.add(i)
            pending.update(edges[i])
        imports[owner] = imported
    return {"units": len(members), "owners": owners, "imports": imports, "copied": copied, "fallback": None}


def coarsen(edges, groups, costs, count):
    attempts = []
    while True:
        attempts.append(count)
        result = one_attempt(edges, groups, costs, count)
        if count <= 2 or result["fallback"] != "budget":
            return result, attempts
        count = (count + 1) // 2


def oracle(edges, groups, costs, count):
    n = len(edges)
    reach = [[i == j or j in edges[i] for j in range(n)] for i in range(n)]
    for k in range(n):
        for i in range(n):
            if reach[i][k]:
                reach[i] = [x or y for x, y in zip(reach[i], reach[k])]
    owners = [g % max(1, count) for g in groups]
    imports = {}
    for owner in sorted(set(owners)):
        roots = [i for i in range(n) if owners[i] == owner]
        imports[owner] = {j for j in range(n) if owners[j] != owner and any(reach[i][j] for i in roots)}
    return imports, sum(costs[i] for ids in imports.values() for i in ids)


def check(edges, groups, costs, count):
    before = one_attempt(edges, groups, costs, count)
    after, attempts = coarsen(edges, groups, costs, count)
    assert attempts[0] == count
    assert all(a > b >= 2 for a, b in zip(attempts, attempts[1:]))
    assert len(attempts) <= max(1, count.bit_length())
    if before["fallback"] != "budget":
        assert before == after and attempts == [count]
    if after["fallback"] is None:
        expected, cost = oracle(edges, groups, costs, attempts[-1])
        assert after["imports"] == expected
        assert after["copied"] == cost <= sum(costs)
        assert 2 <= after["units"] <= count
        for owner, imported in expected.items():
            own = {i for i, unit in enumerate(after["owners"]) if unit == owner}
            assert not (own & imported)
            # No direct dependency may be omitted from the complete closure.
            assert all(j in own | imported for i in own | imported for j in edges[i])
    return before, after, attempts


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", type=Path)
    args = p.parse_args()
    checks = 0
    counts = (0, 1, 2, 3, 4, 8)
    # All 512 directed graphs on three labelled vertices, including all loops.
    for bits in range(1 << 9):
        edges = [{j for j in range(3) if bits & (1 << (3 * i + j))} for i in range(3)]
        for groups in ([0, 1, 2], [0, 0, 1], [0, 0, 0]):
            for costs in ([1, 1, 1], [1, 7, 31]):
                for count in counts:
                    check(edges, groups, costs, count)
                    checks += 1
    rng = random.Random(20261109)
    for _ in range(512):
        n = rng.randrange(4, 49)
        edges = [{rng.randrange(n) for _ in range(rng.randrange(0, 5))} for i in range(n)]
        groups = [rng.randrange(0, 12) for _ in range(n)]
        costs = [rng.randrange(1, 1000) for _ in range(n)]
        for count in (2, 3, 5, 8, 16):
            check(edges, groups, costs, count)
            checks += 1
    examples = []
    for name, n, edges in [
        ("independent", 96, [set() for _ in range(96)]),
        ("chain", 96, [{i + 1} if i < 95 else set() for i in range(96)]),
        ("cycle", 96, [{(i + 1) % 96} for i in range(96)]),
        ("binary_tree", 127, [{j for j in (2*i+1, 2*i+2) if j < 127} for i in range(127)]),
    ]:
        before, after, attempts = check(edges, list(range(n)), [1]*n, 8)
        checks += 1
        examples.append({"name": name, "vertices": n, "baseline_units": before["units"],
                         "candidate_units": after["units"], "attempts": attempts,
                         "copy_cost": after["copied"], "copy_bound": n})
    result = {"status": "Pass", "scope": "finite graph model only; not Rust/native/compile-time evidence",
              "checks": checks, "exhaustive_graphs": 512, "random_graphs": 512, "seed": 20261109,
              "examples": examples, "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    text = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()

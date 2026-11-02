#!/usr/bin/env python3
"""Real-source correctness/capacity controls for cut-support; no performance claims."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import subprocess


def invoke(binary, source, out, mode=None):
    command = [str(binary), str(source)] + ([mode] if mode else [])
    result = subprocess.run(command, capture_output=True, text=True, timeout=90)
    receipt = {"command": command, "exit": result.returncode, "stderr": result.stderr,
               "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
               "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "scope": "Functional source/oracle control; timing is not measured"}
    assert result.returncode == 0, (command, result.stderr)
    data = json.loads(result.stdout)
    out.write_text(json.dumps(data, indent=2) + "\n")
    out.with_suffix(".command.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return data


def bit(row, index):
    return row[index // 64] >> (index % 64) & 1


def member(data, product, row):
    for identifier in product:
        factor = data["factors"][identifier]
        indices = data["component_bits"][factor["group"]]
        if not any(all(bit(value, i) == bit(row, i) for i in indices) for value in factor["rows"]):
            return False
    return True


def projection(data, product, fields):
    coordinates = [data["physical_fields"][name][0] for name in fields]
    partial = {()}
    for identifier in product:
        factor = data["factors"][identifier]
        support = set(data["component_bits"][factor["group"]])
        selected = [i for i in coordinates if i in support]
        if not selected:
            if not factor["rows"]:
                return set()
            continue
        choices = {tuple((i, bit(row, i)) for i in selected) for row in factor["rows"]}
        partial = {a + b for a in partial for b in choices}
    return {tuple(dict(value)[i] for i in coordinates) for value in partial}


def controls(data, n):
    initial = data["relations"]["point:0"][0]
    base = [0] * ((data["physical_bits"] + 63) // 64)
    for identifier in initial:
        row = data["factors"][identifier]["rows"][0]
        base = [a | b for a, b in zip(base, row)]
    fields = [f"b:L{i}" for i in range(n)]
    for name in fields:
        assert data["physical_fields"][name][1] == 1
        assert projection(data, initial, [name]) == {(0,), (1,)}
    chosen = fields[:min(4, n)]
    assert projection(data, initial, chosen) == set(itertools.product([0, 1], repeat=len(chosen)))
    rng = random.Random(20261031 + n)
    for _ in range(100):
        row = list(base)
        for name in fields:
            index = data["physical_fields"][name][0]
            value = rng.randrange(2)
            row[index // 64] = row[index // 64] & ~(1 << (index % 64)) | value << (index % 64)
        assert member(data, initial, row)
    negative = False
    for identifier in initial:
        factor = data["factors"][identifier]
        for index in data["component_bits"][factor["group"]]:
            values = {bit(row, index) for row in factor["rows"]}
            if len(values) == 1:
                row = list(base)
                row[index // 64] ^= 1 << (index % 64)
                assert not member(data, initial, row)
                negative = True
                break
        if negative:
            break
    assert negative
    return {"single_field_projections": n, "four_field_product_rows": 2 ** len(chosen),
            "membership_positive": 100, "membership_negative": 1,
            "initial_joint_rows": str(2 ** n), "factor_count": len(initial)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--graph-prepare", type=Path, required=True)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    cases = []
    for n in [1, 2, 4, 8, 16, 32, 64, 128]:
        source = args.out / f"bool-{n}.zil"
        parameters = ",".join(f"b{i}:bool" for i in range(n))
        arguments = ",".join("false" for _ in range(n))
        source.write_text(f"fn work({parameters})->i32 {{ return 0 }}\nfn main()->i32 {{ return work({arguments}) }}\n")
        cases.append((f"bool-{n}", source, n))
    for n in [1, 4, 8, 16]:
        cases.append((f"sequence-{n}", args.graph_prepare / f"sources/resources-sequential-{n}/main.zil", None))
    cases.extend([
        ("diamond3", args.graph_prepare / "sources/resources-diamond-3/main.zil", None),
        ("bundles", args.repo / "tests/perf/impl071/bundles.zil", None),
        ("loop", args.repo / "tests/perf/impl059/loop.zil", None),
    ])
    for name, source in [
        ("correlated", "fn work(a:bool,b:bool)->bool { let copy=a; return copy }\nfn main()->i32 { if work(true,false) { return 1 }; return 0 }\n"),
        ("fault", "resource T { n:i32 }\nfn consume(give t:T) {}\nfn work(){let a=T{n:1};consume(give a);consume(give a)}\nfn main()->i32 {work();return 0}\n"),
    ]:
        path = args.out / f"{name}.zil"
        path.write_text(source)
        cases.append((name, path, None))
    summary = []
    for name, source, n in cases:
        production = invoke(args.baseline, source, args.out / f"{name}.production.json", "production")
        assert production["outcome"] == ("Rejected" if name == "fault" else "Accepted"), (name, production)
        result = invoke(args.binary, source, args.out / f"{name}.factor.json")
        assert all(f["outcome"] != "Error" for f in result["functions"]), result
        row = {"case": name, "production": production["outcome"], "functions": [
            {key: f[key] for key in ["name", "outcome", "check", "error", "explicit_total_rows_upper_bound"] if key in f}
            for f in result["functions"]]}
        if n:
            function = next(f for f in result["functions"] if f["name"] == "work")
            assert function["outcome"] == "CompleteFactored"
            row["queries"] = controls(function, n)
            old = invoke(args.baseline, source, args.out / f"{name}.baseline.json", "dense:direct:owner:gc")
            original = next(f for f in old["functions"] if f["name"] == "work")
            row["baseline_work"] = original["outcome"]
            if function["check"] == "ExactOracleRows":
                assert original["outcome"] == "Complete"
                assert original["canonical_fingerprint"] == function["canonical_fingerprint"]
            else:
                assert function["canonical_fingerprint"] is None
        summary.append(row)
        (args.out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(name, row, flush=True)

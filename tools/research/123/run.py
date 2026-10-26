#!/usr/bin/env python3
"""123 exact-source complete compilation experiments; no production admission shortcut."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import math
import os
import platform
import random
import shutil
import statistics
import subprocess
import time
from pathlib import Path

from inputs import inputs
from support import ENV, archive_directory, compact_report, extract, inventory, invoke, normalize, sha, write

BASE = "ff0ddb7e59b2b0577c35b92e74b895c86f20b218"
SCRIPT_NAMES = ["run.py", "inputs.py", "support.py", "main.rs", "Cargo.toml", "Cargo.lock"]
COUNTERS = ["types_body_visits", "db_read_calls", "db_dfs_pops", "db_snapshot_reads",
            "types_slow_starts", "types_full_rounds", "analyze_body_visits", "verify_body_visits",
            "relation_body_visits", "fact_nodes", "fact_edges", "fact_pops", "edge_visits",
            "seed_enqueues", "effect_solver_entries", "initialization_transfers"]
PHASES = ["compile_path", "load_sources", "discovery_parse", "parse_execute", "expand_execute",
          "ct_execute", "types_execute", "analyze_execute", "query_lookup", "raw_digest",
          "query_publish", "types_check_program", "types_effect", "analyze", "verify", "lower",
          "build_including_clang", "reserved17", "reserved18", "verify_input_clone", "other_execute"]


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def exact(repo, commit):
    assert subprocess.check_output(["git", "rev-parse", commit + "^{commit}"], cwd=repo, text=True).strip() == commit
    assert len(commit) == 40


def tools_identity():
    paths = {name: str(Path(shutil.which(name)).resolve()) for name in ["cargo", "rustc", "python3"]}
    paths["clang"] = ENV["ZIL_CLANG"]
    return {name: dict(path=path, sha256=sha(path)) for name, path in paths.items()}


def prepare(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    for commit in [BASE, args.candidate, args.harness]:
        exact(repo, commit)
    for name in SCRIPT_NAMES:
        assert subprocess.check_output(["git", "show", args.harness + ":tools/research/123/" + name], cwd=repo) == Path(__file__).with_name(name).read_bytes()
    cases = inputs(repo, out)
    variants = {"BASE": BASE, "ON": args.candidate}
    toolset = tools_identity()
    write(out / "preregistered-inputs.json", dict(variants=variants, cases=cases, harness=args.harness,
        input_sha256=inventory(out / "inputs"), edits_sha256=inventory(out / "edit-manifests"),
        historical_cells=19, historical_endpoints=95, growth_sizes=[16, 32, 64],
        scope="finite exploration and functionality; any admission requires a separate frozen confirmation"))
    for arm, commit in variants.items():
        for kind in ["plain", "diag"]:
            key = arm + "-" + kind
            tree = out / ("source-" + key)
            tree.mkdir()
            extract(repo, commit, ["Cargo.toml", "Cargo.lock", "crates"], tree)
            extract(repo, args.harness, ["tools/research/123", "tools/research/101/probe.py", "tools/research/089"], tree)
            if kind == "diag":
                spec = importlib.util.spec_from_file_location("probe123", tree / "tools/research/101/probe.py")
                probe = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(probe)
                probe.instrument(tree, diagnostic=True)
                path = tree / "crates/zil-analyze/src/lib.rs"
                path.write_text(probe.at_entry(path.read_text(), "fn transfer(", probe.add(15)))
            write(out / ("source-" + key + "-sha256.json"), inventory(tree))
            target = out / ("target-" + key)
            flags = "--check-cfg=cfg(zil101_count) --check-cfg=cfg(zil101_diag)" + (" --cfg zil101_diag" if kind == "diag" else "")
            invoke([toolset["cargo"]["path"], "build", "--offline", "--locked", "--release", "--manifest-path",
                    tree / "tools/research/123/Cargo.toml", "--target-dir", target], tree, out / ("logs/build-" + key),
                   {"RUSTFLAGS": flags, "ZIL123_PRODUCTION_IDENTITY": commit, "RUSTC": toolset["rustc"]["path"]})
            dest = out / "bin" / key
            dest.parent.mkdir(exist_ok=True)
            shutil.copy2(target / "release/pipeline123", dest)
            write(out / ("logs/prune-" + key + ".json"), dict(path=str(target), retained_binary=str(dest), sha256=sha(dest)))
            shutil.rmtree(target)
            print("built", key, flush=True)
    write(out / "identity.json", dict(domain="ZIL-123-COMPLETE-CFG-COST-1", variants=variants, harness=args.harness,
        cases=cases, schema=17, scripts={name: sha(Path(__file__).with_name(name)) for name in SCRIPT_NAMES},
        binaries={f"{a}-{k}": sha(out / "bin" / f"{a}-{k}") for a in variants for k in ["plain", "diag"]},
        source_manifests={f"{a}-{k}": sha(out / f"source-{a}-{k}-sha256.json") for a in variants for k in ["plain", "diag"]},
        input_sha256=inventory(out / "inputs"), edits_sha256=inventory(out / "edit-manifests"), tools=toolset,
        execution_env=ENV, host=platform.platform(), counters=COUNTERS, phases=PHASES,
        rustc=subprocess.check_output([toolset["rustc"]["path"], "-Vv"], text=True),
        clang=subprocess.check_output([toolset["clang"]["path"], "--version"], text=True),
        admission="NotRun", profile="release codegen-units1 debug0 incremental0 jobs1"))


def verify(out):
    identity = json.loads((out / "identity.json").read_text())
    assert identity["domain"] == "ZIL-123-COMPLETE-CFG-COST-1"
    assert tools_identity() == identity["tools"] and ENV == identity["execution_env"]
    for name, digest in identity["scripts"].items():
        assert sha(Path(__file__).with_name(name)) == digest, name
    for key, digest in identity["binaries"].items():
        assert sha(out / "bin" / key) == digest
        manifest = out / ("source-" + key + "-sha256.json")
        assert sha(manifest) == identity["source_manifests"][key]
        assert inventory(out / ("source-" + key)) == json.loads(manifest.read_text())
    assert inventory(out / "inputs") == identity["input_sha256"]
    assert inventory(out / "edit-manifests") == identity["edits_sha256"]
    return identity


def request(out, identity, arm, case, mode, tag, kind="plain", native=False):
    folder = out / tag / (case["name"] + "-" + mode + "-" + arm + "-" + kind)
    source = out / "inputs" / case["edit"]
    before = sha(source)
    command = invoke([out / "bin" / (arm + "-" + kind), folder, mode, out / "inputs" / case["root"],
                      out / "edit-manifests" / case["name"], case["capacity"]],
                     out, folder.parent / (folder.name + "-runner"), identity["execution_env"], timeout=180, required=False)
    assert sha(source) == before, "the complete request must restore edits even on rejection"
    report_path = folder / "report.json"
    if not report_path.exists():
        return dict(case=case["name"], mode=mode, arm=arm, kind=kind, complete=False, command=command,
                    report_path=None, failure="NoReport"), None
    data = json.loads(report_path.read_text())
    assert data["production_identity"] == identity["variants"][arm] and data["schema"] == 17
    assert data["mode"] == mode and data["capacity"] == case["capacity"]
    assert data["diagnostic"] is (kind == "diag") and data["counting"] is False
    assert [r["scenario"] for r in data["records"]] == case["scenarios"]
    controls = []
    for row in data["records"]:
        control = normalize(row)
        if native and row["report"]["status"] == "Success" and "executable" in row["report"]["artifacts"]:
            prefix = folder / (row["scenario"] + "-native")
            execution = invoke([row["report"]["artifacts"]["executable"]], out, prefix, identity["execution_env"],
                               stdin=case["stdin"].encode(), timeout=20)
            expected = case.get("stdout_by_scenario", {}).get(row["scenario"], case["stdout"])
            assert prefix.with_suffix(".stdout").read_bytes() == expected.encode()
            assert prefix.with_suffix(".stderr").read_bytes() == b""
            control["native"] = dict(exit=execution["exit"], stdout_sha256=sha(prefix.with_suffix(".stdout")))
        controls.append(control)
    row = dict(case=case["name"], family=case["family"], n=case.get("n"), mode=mode, arm=arm, kind=kind,
               complete=data["complete"], command=command, report_path=str(report_path) + ".gz",
               process_wall_ns=command["elapsed_ns"], vmhwm_kib=data["vmhwm_kib"], session_drop_ns=data["session_drop_ns"],
               controls_sha256=hashlib.sha256(canonical(controls)).hexdigest(), records=[{key: r[key] for key in
                   ["scenario", "api_wall_ns", "counts", "phase_ns", "phase_calls"]} | {"status": r["report"]["status"]} for r in data["records"]])
    assert data["complete"] is (command["exit"] == 0)
    if data["complete"]:
        assert all(r["report"]["status"] == "Success" and r["report"]["verification_runs"] == 1 and r["active_queries"] == 0 for r in data["records"])
    write(folder / "control-hash.json", dict(sha256=row["controls_sha256"], normalization="time/probe/path-independent emitted bytes; exact public report and sources retained"))
    compact_report(report_path)
    return row, controls


def functional(args):
    out = args.out.resolve()
    identity = verify(out)
    assert not (out / "functional-start.json").exists()
    write(out / "functional-start.json", dict(identity_sha256=sha(out / "identity.json"), source=identity["variants"]))
    rows, cells = [], []
    with (out / "functional-journal.jsonl").open("x") as journal:
        for case in identity["cases"]:
            for mode in case["modes"]:
                controls = {}
                for arm in ["BASE", "ON"]:
                    for kind in ["plain", "diag"]:
                        row, control = request(out, identity, arm, case, mode + "-emit", "functional", kind,
                                               native=mode == "build" and kind == "plain")
                        rows.append(row)
                        journal.write(json.dumps(row, sort_keys=True) + "\n")
                        journal.flush()
                        controls[arm + "-" + kind] = [{k:v for k,v in r.items() if k != "native"} for r in control] if control else None
                        archive_directory(out / "functional" / (case["name"] + "-" + mode + "-emit-" + arm + "-" + kind))
                complete = all(r["complete"] for r in rows[-4:])
                if complete:
                    assert all(v == controls["BASE-plain"] for v in controls.values()), (case["name"], mode, "full emitted/public observations differ")
                else:
                    assert case["family"] != "historical", (case["name"], mode, "historical case rejected")
                cells.append(dict(case=case["name"], mode=mode, complete=complete, status="Pass" if complete else "CapacityOrFailure", arms={r["arm"] + "-" + r["kind"]: r["complete"] for r in rows[-4:]}))
                write(out / "functional.json", dict(cells=cells, rows=rows, admission="NotRun"))
                print("functional", case["name"], mode, cells[-1]["status"], flush=True)


def schedule(cases, blocks):
    rng = random.Random(123)
    cells = [(case["name"], mode) for case in cases for mode in case["modes"]]
    rng.shuffle(cells)
    rows = []
    for case, mode in cells:
        rows.append(dict(case=case, mode=mode, block=-1, warmup=True, order=["BASE", "ON", "ON", "BASE"]))
        orders = [False] * (blocks // 2) + [True] * (blocks // 2)
        rng.shuffle(orders)
        for block, reverse in enumerate(orders):
            rows.append(dict(case=case, mode=mode, block=block, warmup=False,
                             order=["ON", "BASE", "BASE", "ON"] if reverse else ["BASE", "ON", "ON", "BASE"]))
    return rows


def explore(args):
    out = args.out.resolve()
    identity = verify(out)
    assert not (out / "exploration-protocol.json").exists()
    assert args.blocks > 0 and args.blocks % 2 == 0
    functional = json.loads((out / "functional.json").read_text())
    incomplete = [c for c in functional["cells"] if not c["complete"]]
    comparable = [c for c in identity["cases"] if all(v["complete"] for v in functional["cells"] if v["case"] == c["name"])]
    plan = schedule(comparable, args.blocks)
    write(out / "exploration-schedule.json", plan)
    write(out / "exploration-protocol.json", dict(domain="ZIL-123-EXPLORATION-1", identity_sha256=sha(out / "identity.json"),
        schedule_sha256=sha(out / "exploration-schedule.json"), blocks=args.blocks,
        historical_cells=19, endpoints_per_cell=["process", "cold", "unchanged", "edit", "second_edit"],
        noncomparables=incomplete, reason="failures are retained as capacity/correctness outcomes, never speed ratios",
        weights="equal 1/19 across historical process cells; equal 1/28 across all predeclared cells only if all complete",
        interval="exploratory medians and raw blocks only; no confidence/admission claim",
        stopping="fixed blocks once; any semantic mismatch stops and retains the prefix; no retries to pass",
        affinity=sorted(os.sched_getaffinity(0)), shared_environment=True, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())))
    cases = {c["name"]: c for c in identity["cases"]}
    rows = []
    expected = {}
    with (out / "exploration-samples.jsonl").open("x") as journal:
        for cell in plan:
            key = (cell["case"], cell["mode"])
            for position, arm in enumerate(cell["order"]):
                row, control = request(out, identity, arm, cases[cell["case"]], cell["mode"],
                    f"exploration/{cell['case']}-{cell['mode']}/b{cell['block']}/p{position}")
                assert row["complete"], (cell, "request failed during timing; prefix preserved")
                digest = row["controls_sha256"]
                if key not in expected:
                    expected[key] = digest
                assert digest == expected[key], (cell, "semantic observations changed")
                row.update(block=cell["block"], warmup=cell["warmup"], position=position)
                journal.write(json.dumps(row, sort_keys=True) + "\n")
                journal.flush()
                rows.append(row)
            print("explore", cell["case"], cell["mode"], cell["block"], flush=True)
    write(out / "exploration-samples.json", rows)
    analyze(out, rows)


def analyze(out, rows):
    endpoints = []
    kept = [r for r in rows if not r["warmup"]]
    for case, mode in sorted({(r["case"], r["mode"]) for r in kept}):
        cell = [r for r in kept if (r["case"], r["mode"]) == (case, mode)]
        for endpoint in ["process", *[r["scenario"] for r in cell[0]["records"]]]:
            base, on, ratios = [], [], []
            cost = lambda row: row["process_wall_ns"] if endpoint == "process" else next(r["api_wall_ns"] for r in row["records"] if r["scenario"] == endpoint)
            for block in sorted({r["block"] for r in cell}):
                a = statistics.mean(cost(r) for r in cell if r["block"] == block and r["arm"] == "BASE")
                b = statistics.mean(cost(r) for r in cell if r["block"] == block and r["arm"] == "ON")
                assert a > 0 and b > 0
                base.append(a); on.append(b); ratios.append(b / a)
            endpoints.append(dict(case=case, mode=mode, family=cell[0]["family"], n=cell[0]["n"], endpoint=endpoint,
                                  base_median_ns=statistics.median(base), on_median_ns=statistics.median(on),
                                  ratio=statistics.median(ratios), block_ratios=ratios))
    historical = [e for e in endpoints if e["family"] == "historical"]
    assert len(historical) == 95
    process = [e for e in historical if e["endpoint"] == "process"]
    write(out / "exploration-summary.json", dict(scope="finite shared-environment exploration; no admission conclusion",
        endpoints=endpoints, historical_endpoints=95, historical_process_geomean=math.exp(statistics.mean(math.log(e["ratio"]) for e in process)),
        historical_above_1_01=sum(e["ratio"] > 1.01 for e in historical), admission="NotRun",
        samples_sha256=sha(out / "exploration-samples.json"), protocol_sha256=sha(out / "exploration-protocol.json")))


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("research assertions require ordinary Python, not -O")
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--repo", type=Path, required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--harness", required=True)
    p.add_argument("--out", type=Path, required=True)
    for name in ["verify", "functional", "explore"]:
        p = sub.add_parser(name)
        p.add_argument("--out", type=Path, required=True)
        if name == "explore":
            p.add_argument("--blocks", type=int, default=4)
    args = parser.parse_args()
    if args.operation == "verify":
        verify(args.out.resolve())
    else:
        globals()[args.operation](args)

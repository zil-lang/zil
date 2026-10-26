#!/usr/bin/env python3
"""One sealed confirmation: complete requests, fixed family, no retry-to-pass."""
import argparse
import gzip
import hashlib
import json
import math
import os
import statistics
import subprocess
import time
from pathlib import Path
import run

BLOCKS, FAMILY, MARGIN = 64, 142, 1.01


def sources_match(records, case, identity, out):
    assert [r["scenario"] for r in records] == case["scenarios"]
    for record in records:
        expected = dict(identity["input_sha256"])
        scenario = record["scenario"]
        if scenario.endswith("_edit") or scenario == "body_restore":
            expected[case["edit"]] = identity["edits_sha256"][case["name"] + "/" + scenario + ".zil"]
        assert record["sources"]
        for source in record["sources"]:
            relative = str(Path(source["path"]).relative_to(out / "inputs"))
            assert source["sha256"] == expected[relative], (case["name"], scenario, relative)


def frozen(out):
    identity = run.verify(out)
    protocol = json.loads((out / "confirmation-protocol.json").read_text())
    assert protocol["domain"] == "ZIL-123-CONFIRMATION-1"
    assert run.sha(__file__) == protocol["controller_sha256"]
    assert run.sha(out / "identity.json") == protocol["identity_sha256"]
    assert run.sha(out / "functional.json") == protocol["functional_sha256"]
    assert run.sha(out / "functional-anchor.json") == protocol["functional_anchor_sha256"]
    assert run.sha(out / "confirmation-schedule.json") == protocol["schedule_sha256"]
    if protocol["functional_reuse_sha256"] is not None:
        assert run.sha(out / "functional-reuse.json") == protocol["functional_reuse_sha256"]
    for field, expected in dict(blocks=BLOCKS, family=FAMILY, margin=MARGIN, processes=7280, requests=29120).items():
        assert type(protocol[field]) is type(expected) and protocol[field] == expected
    schedule = json.loads((out / "confirmation-schedule.json").read_text())
    assert schedule == run.schedule(identity["cases"], BLOCKS)
    return identity, protocol, schedule


def freeze(args):
    out, repo = args.out.resolve(), Path(__file__).resolve().parents[3]
    identity = run.verify(out)
    run.exact(repo, args.controller)
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/confirm.py"], cwd=repo) == Path(__file__).read_bytes()
    assert not (out / "confirmation-protocol.json").exists()
    functional = json.loads((out / "functional.json").read_text())
    expected = [(c["name"], m) for c in identity["cases"] for m in c["modes"]]
    assert len(expected) == 28 and [(c["case"], c["mode"]) for c in functional["cells"]] == expected
    assert all(c["complete"] and c["status"] == "Pass" for c in functional["cells"])
    assert len(functional["rows"]) == 112
    assert all(r["complete"] is True for r in functional["rows"])
    anchor = json.loads((out / "functional-anchor.json").read_text())
    assert anchor["status"] == "Pass" and anchor["processes"] == 56 and anchor["requests"] == 224 and anchor["native_executions"] == 48
    assert anchor["identity_sha256"] == run.sha(out / "identity.json")
    assert anchor["rows_sha256"] == run.sha(out / "functional-anchor-rows.json")
    plan = run.schedule(identity["cases"], BLOCKS)
    run.write(out / "confirmation-schedule.json", plan)
    run.write(out / "confirmation-protocol.json", dict(domain="ZIL-123-CONFIRMATION-1",
        controller_commit=args.controller, controller_sha256=run.sha(__file__), identity_sha256=run.sha(out / "identity.json"),
        functional_sha256=run.sha(out / "functional.json"), schedule_sha256=run.sha(out / "confirmation-schedule.json"),
        functional_anchor_sha256=run.sha(out / "functional-anchor.json"),
        functional_reuse_sha256=run.sha(out / "functional-reuse.json") if (out / "functional-reuse.json").exists() else None,
        exploration_sha256=run.sha(out / "exploration-summary.json"), variants=identity["variants"],
        blocks=BLOCKS, family=FAMILY, margin=MARGIN, alpha=0.05, processes=7280, requests=29120,
        old_cells=19, old_endpoints=95, growth_cells=9, growth_endpoints=45, warm_processes=112, retained_processes=7168,
        layout="each mode: one 4-process warm block; 64 balanced ABBA/BAAB blocks; seed123; no replacement samples",
        family_rule="95 historical + 45 growth + equal-weight 19-process and 28-process aggregates = 142",
        interval="median of 64 within-block ON/BASE mean-cost ratios; exact binomial order18/47 with 142-family Bonferroni",
        aggregate="each block geometric mean of its per-cell process ratios; median and same order18/47; weights fixed 1/19 and 1/28",
        stopping="fixed schedule once; any failed request or changed full semantic observations stops, retaining all attempted raw evidence",
        admission="any endpoint lower>1.01 is NoGo; every endpoint upper<=1.01 and both aggregate upper<1 is necessary for eligibility; otherwise Inconclusive",
        growth_failures="not dropped or converted to speed ratios; incomplete functional growth prevents this full confirmation from starting",
        affinity_cpu=2, required_window="root coordinated no builds/tests/native verification; root may perform light static/document work"))


def sample(args):
    out = args.out.resolve()
    identity, protocol, plan = frozen(out)
    assert sorted(os.sched_getaffinity(0)) == [protocol["affinity_cpu"]]
    assert not (out / "confirmation-start.json").exists()
    run.write(out / "confirmation-start.json", dict(window=args.window, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        protocol_sha256=run.sha(out / "confirmation-protocol.json"), affinity=sorted(os.sched_getaffinity(0))))
    cases = {c["name"]: c for c in identity["cases"]}
    rows, last = [], time.monotonic()
    references = json.loads((out / "functional-anchor.json").read_text())["references"]
    with (out / "confirmation-samples.jsonl").open("x") as journal:
        for cell in plan:
            key = (cell["case"], cell["mode"])
            for position, arm in enumerate(cell["order"]):
                tag = f"confirmation/{cell['case']}-{cell['mode']}/b{cell['block']}/p{position}"
                case = cases[cell["case"]]
                edit = out / "inputs" / case["edit"]
                before = run.sha(edit)
                assert before == identity["input_sha256"][case["edit"]], (cell, "frozen input drift before request")
                row, controls = run.request(out, identity, arm, case, cell["mode"], tag)
                row["input_guard"] = dict(path=str(edit), before=before, after=run.sha(edit), expected=identity["input_sha256"][case["edit"]])
                row.update(block=cell["block"], warmup=cell["warmup"], position=position)
                journal.write(json.dumps(row, sort_keys=True) + "\n")
                journal.flush()
                rows.append(row)
                assert row["complete"], (cell, position, "request failed")
                assert row["input_guard"]["after"] == before, (cell, "input drift after request")
                sources_match(controls, case, identity, out)
                assert row["controls_sha256"] == references[":".join(key)]["sha256"], (cell, position, "semantic observation differs from functional anchor")
            if time.monotonic() - last >= 25:
                print("confirmation", len(rows), "/ 7280", cell["case"], cell["mode"], cell["block"], flush=True)
                last = time.monotonic()
    assert len(rows) == 7280 and sum(r["warmup"] for r in rows) == 112
    run.write(out / "confirmation-samples.json", rows)
    run.write(out / "confirmation-finish.json", dict(status="Pass", scope="acquisition only; admission is separate", window=args.window,
        processes=7280, requests=29120, samples_sha256=run.sha(out / "confirmation-samples.json"),
        journal_sha256=run.sha(out / "confirmation-samples.jsonl"), protocol_sha256=run.sha(out / "confirmation-protocol.json"),
        utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())))
    print("confirmation acquisition complete; release exclusive window", flush=True)


def sealed_rows(out, identity, protocol, plan):
    finish = json.loads((out / "confirmation-finish.json").read_text())
    start = json.loads((out / "confirmation-start.json").read_text())
    assert finish["status"] == "Pass" and finish["processes"] == 7280 and finish["requests"] == 29120
    assert finish["protocol_sha256"] == run.sha(out / "confirmation-protocol.json")
    assert start["protocol_sha256"] == finish["protocol_sha256"] and start["window"] == finish["window"]
    assert start["affinity"] == [protocol["affinity_cpu"]] and start["utc"] <= finish["utc"]
    for filename, field in [("confirmation-samples.json", "samples_sha256"), ("confirmation-samples.jsonl", "journal_sha256")]:
        assert run.sha(out / filename) == finish[field]
    rows = json.loads((out / "confirmation-samples.json").read_text())
    assert rows == [json.loads(s) for s in (out / "confirmation-samples.jsonl").read_text().splitlines()]
    expected = [(cell, position, arm) for cell in plan for position, arm in enumerate(cell["order"])]
    assert len(rows) == len(expected) == 7280
    references = json.loads((out / "functional-anchor.json").read_text())["references"]
    cases = {c["name"]: c for c in identity["cases"]}
    seen = set()
    for row, (cell, position, arm) in zip(rows, expected):
        for name in ["case", "mode", "block", "warmup"]:
            assert type(row[name]) is type(cell[name]) and row[name] == cell[name]
        assert row["position"] == position and row["arm"] == arm and row["complete"] is True and row["kind"] == "plain"
        path = Path(row["report_path"])
        tag = f"confirmation/{cell['case']}-{cell['mode']}/b{cell['block']}/p{position}"
        assert path == out / tag / (cell["case"] + "-" + cell["mode"] + "-" + arm + "-plain") / "report.json.gz"
        assert path not in seen
        seen.add(path)
        data = json.loads(gzip.decompress(path.read_bytes()))
        assert data["production_identity"] == identity["variants"][arm] and data["schema"] == 17 and data["complete"] is True
        assert data["mode"] == cell["mode"] and data["counting"] is False and data["diagnostic"] is False
        command_path = path.parent.parent / (path.parent.name + "-runner.command.json")
        command = json.loads(command_path.read_text())
        assert command == row["command"] and type(command["exit"]) is int and command["exit"] == 0
        case = cases[cell["case"]]
        assert row["family"] == case["family"] and row["n"] == case.get("n")
        assert row["vmhwm_kib"] == data["vmhwm_kib"] and row["session_drop_ns"] == data["session_drop_ns"]
        assert row["input_guard"] == dict(path=str(out / "inputs" / case["edit"]),
            before=identity["input_sha256"][case["edit"]], after=identity["input_sha256"][case["edit"]], expected=identity["input_sha256"][case["edit"]])
        assert command["argv"] == list(map(str, [out / "bin" / (arm + "-plain"), path.parent, cell["mode"],
            out / "inputs" / case["root"], out / "edit-manifests" / case["name"], case["capacity"]]))
        assert command["cwd"] == str(out) and "outcome" not in command
        assert command_path.with_suffix("").with_suffix(".stderr").read_bytes() == b""
        assert command_path.with_suffix("").with_suffix(".stdout").read_text() == f"pipeline123 {cell['mode']} {path.parent} complete=true\n"
        assert [r["scenario"] for r in data["records"]] == case["scenarios"]
        sources_match(data["records"], case, identity, out)
        assert all(r["report"]["verification_runs"] == 1 and r["active_queries"] == 0 for r in data["records"])
        assert type(row["process_wall_ns"]) is int and row["process_wall_ns"] > 0 and row["process_wall_ns"] == command["elapsed_ns"]
        observed = [{k:r[k] for k in ["scenario", "api_wall_ns", "counts", "phase_ns", "phase_calls"]} | {"status":r["report"]["status"]} for r in data["records"]]
        assert row["records"] == observed
        assert all(type(r["api_wall_ns"]) is int and r["api_wall_ns"] > 0 and r["status"] == "Success" and r["counts"] == [0]*16 and r["phase_ns"] == [0]*21 and r["phase_calls"] == [0]*21 and all(type(x) is int for x in r["counts"] + r["phase_ns"] + r["phase_calls"]) for r in observed)
        control = [run.normalize(r) for r in data["records"]]
        assert sum(r["api_wall_ns"] for r in observed) + row["session_drop_ns"] <= row["process_wall_ns"]
        digest = hashlib.sha256(run.canonical(control)).hexdigest()
        assert digest == row["controls_sha256"]
        key = (row["case"], row["mode"])
        assert references[":".join(key)]["sha256"] == digest
    return rows


def analyze(args):
    out = args.out.resolve()
    identity, protocol, plan = frozen(out)
    assert not (out / "confirmation-summary.json").exists()
    rows = sealed_rows(out, identity, protocol, plan)
    kept = [r for r in rows if not r["warmup"]]
    endpoints = []
    interval = lambda values: dict(point=statistics.median(values), lower=sorted(values)[17], upper=sorted(values)[46])
    for case, mode in sorted({(r["case"], r["mode"]) for r in kept}):
        cell = [r for r in kept if (r["case"], r["mode"]) == (case, mode)]
        for endpoint in ["process", *[r["scenario"] for r in cell[0]["records"]]]:
            costs = lambda r: r["process_wall_ns"] if endpoint == "process" else next(x["api_wall_ns"] for x in r["records"] if x["scenario"] == endpoint)
            base, on, ratios = [], [], []
            for block in range(BLOCKS):
                a = statistics.mean(costs(r) for r in cell if r["block"] == block and r["arm"] == "BASE")
                b = statistics.mean(costs(r) for r in cell if r["block"] == block and r["arm"] == "ON")
                assert a > 0 and b > 0
                base.append(a); on.append(b); ratios.append(b / a)
            endpoints.append(dict(case=case, mode=mode, family=cell[0]["family"], n=cell[0]["n"], endpoint=endpoint,
                base_median_ns=statistics.median(base), on_median_ns=statistics.median(on), block_ratios=ratios, **interval(ratios),
                first_half=statistics.median(ratios[:32]), second_half=statistics.median(ratios[32:]),
                first_arm={a:statistics.median(ratios[r["block"]] for r in cell if r["position"] == 0 and r["arm"] == a) for a in ["BASE", "ON"]}))
    assert len(endpoints) == 140 and sum(e["family"] == "historical" for e in endpoints) == 95
    aggregates = []
    for name, cells in [("historical19", [e for e in endpoints if e["endpoint"] == "process" and e["family"] == "historical"]),
                        ("all28", [e for e in endpoints if e["endpoint"] == "process"])]:
        ratios = [math.exp(statistics.mean(math.log(e["block_ratios"][b]) for e in cells)) for b in range(BLOCKS)]
        aggregates.append(dict(name=name, weight=1/len(cells), block_ratios=ratios, **interval(ratios)))
    failures = [e for e in endpoints if e["lower"] > MARGIN]
    unknown = [e for e in endpoints if e["upper"] > MARGIN]
    status = "NoGo" if failures else ("EligiblePendingReview" if not unknown and all(a["upper"] < 1 for a in aggregates) else "Inconclusive")
    run.write(out / "confirmation-summary.json", dict(status=status, endpoints=endpoints, aggregates=aggregates,
        family=FAMILY, old_endpoints=95, growth_endpoints=45, above_margin_point=sum(e["point"] > MARGIN for e in endpoints),
        lower_above_margin=len(failures), upper_above_margin=len(unknown), processes=7280, requests=29120,
        peak_parent_rss_kib={a:max(r["vmhwm_kib"] for r in rows if r["arm"] == a) for a in ["BASE", "ON"]},
        memory_scope="parent pipeline process VmHWM only, excluding clang child; not target runtime memory",
        protocol_sha256=run.sha(out / "confirmation-protocol.json"), samples_sha256=run.sha(out / "confirmation-samples.json"),
        warning="nominal family interval assumes stable sufficiently independent blocks; no universal complexity or Rust-product claim"))
    print(status, "upper_above_margin", len(unknown), "lower_above_margin", len(failures), flush=True)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation", required=True)
    for name in ["freeze", "sample", "analyze"]:
        p = sub.add_parser(name)
        p.add_argument("--out", type=Path, required=True)
        if name == "freeze": p.add_argument("--controller", required=True)
        if name == "sample": p.add_argument("--window", required=True)
    args = parser.parse_args()
    globals()[args.operation](args)

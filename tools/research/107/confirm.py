#!/usr/bin/env python3
"""One fixed BASE/ON confirmation after independent measurement calibration."""
from __future__ import annotations
import argparse, gzip, hashlib, json, math, os, random, statistics, time
from pathlib import Path
import run as prepared
from support import sha, write

BLOCKS, FAMILY, MARGIN, ALPHA = 64, 95, 1.01, .05
CALIBRATION_SHA = "d0d7de73414fc632b20e3c72c2e9d10b3dc4ad2efeb773ca77ad2b8112bbd4cd"
ARMS = ["BASE", "ON"]


def record_metadata(record):
    row={k:record[k] for k in ["scenario","api_wall_ns","counts","active_queries","cached_queries","observation_ns"]}
    row.update({k:record["report"][k] for k in ["status","phase","verification_runs"]})
    return row


def semantic_digest(record):
    # Equal to the existing broad semantic comparison, excluding instrumentation,
    # timings and output locations. LLVM is checked independently below.
    skip={"report","api_wall_ns","counts","phase_ns","phase_calls","observation_ns","raw_snapshots"}
    value={k:v for k,v in record.items() if k not in skip}
    value["report"]={k:v for k,v in record["report"].items() if k not in ["seconds","commands","artifacts"]}
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()


def report_folder(out, cell, position, variant):
    block="warm" if cell["warmup"] else f"b{cell['block']:02}"
    tag=f"confirmation/{cell['case']}-{cell['mode']}/{block}/p{position}"
    return out/tag/f"{cell['case']}-{cell['mode']}-{variant}"


def integer(value, minimum):
    assert type(value) is int and value>=minimum, ("invalid integer metric",value,minimum)


def equal_json(left,right):
    if type(left) is not type(right):return False
    if type(left) is dict:return left.keys()==right.keys() and all(equal_json(left[k],right[k]) for k in left)
    if type(left) is list:return len(left)==len(right) and all(equal_json(a,b) for a,b in zip(left,right))
    return left==right


def validate_observations(rows, frozen_schedule, cases, out):
    expected=[(cell,position,variant) for cell in frozen_schedule for position,variant in enumerate(cell["order"])]
    assert len(rows)==len(expected)==4940
    case_map={c["name"]:c for c in cases}
    seen=set()
    for row,(cell,position,variant) in zip(rows,expected):
        wanted={k:cell[k] for k in ["case","mode","block","warmup"]}|dict(variant=variant,position=position)
        for key,value in wanted.items():
            assert type(row[key]) is type(value) and row[key]==value,("schedule mismatch",key,row.get(key),value)
        folder=report_folder(out,cell,position,variant)
        assert row["report_path"]==str(folder/"report.json.gz")
        assert row["report_path"] not in seen;seen.add(row["report_path"])
        integer(row["process_wall_ns"],1);integer(row["session_drop_ns"],0)
        if row["vmhwm_kib"] is not None:integer(row["vmhwm_kib"],1)
        records=row["records"]
        assert type(records) is list and len(records)==4
        assert [r["scenario"] for r in records]==case_map[cell["case"]]["scenarios"]
        for r in records:
            integer(r["api_wall_ns"],1);integer(r["observation_ns"],0)
            integer(r["active_queries"],0);integer(r["cached_queries"],0);integer(r["verification_runs"],1)
            assert r["active_queries"]==0 and r["verification_runs"]==1 and r["status"]=="Success"
            assert r["phase"]==("verify" if cell["mode"]=="verify" else "link")
            assert type(r["counts"]) is list and len(r["counts"])==14
            assert all(type(c) is int and c==0 for c in r["counts"]),"timed counter payload changed"
        assert sum(r["api_wall_ns"] for r in records)<=row["process_wall_ns"],"process shorter than contained API work"


def load_sealed_rows(out):
    protocol_sha=sha(out/"confirmation-protocol.json")
    start=json.loads((out/"confirmation-start.json").read_text())
    finish=json.loads((out/"confirmation-finish.json").read_text())
    assert start["protocol_sha256"]==finish["protocol_sha256"]==protocol_sha
    assert type(start["window"]) is str and start["window"] and start["window"]==finish["window"]
    assert equal_json(start["affinity"],[2]) and finish["status"]=="Pass"
    for key,value in dict(processes=4940,requests=19760,warmup_processes=76,retained_processes=4864).items():
        assert type(finish[key]) is int and finish[key]==value
    assert finish["samples_sha256"]==sha(out/"confirmation-samples.json")
    assert finish["journal_sha256"]==sha(out/"confirmation-samples.jsonl")
    rows=json.loads((out/"confirmation-samples.json").read_text())
    journal=[json.loads(line) for line in (out/"confirmation-samples.jsonl").read_text().splitlines()]
    assert equal_json(rows,journal) and len(rows)==4940 and sum(r["warmup"] is True for r in rows)==76
    return rows


def validate_capture(out, row, identity, protocol):
    case=next(c for c in identity["cases"] if c["name"]==row["case"])
    folder=Path(row["report_path"]).parent
    storage=json.loads((folder/"report-storage.json").read_text())
    compressed=(folder/"report.json.gz").read_bytes();original=gzip.decompress(compressed)
    assert storage["original"]=="report.json" and storage["compressed"]=="report.json.gz"
    integer(storage["original_bytes"],1)
    assert storage["compressed_sha256"]==hashlib.sha256(compressed).hexdigest()
    assert storage["original_sha256"]==hashlib.sha256(original).hexdigest() and storage["original_bytes"]==len(original)
    data=json.loads(original)
    integer(data["schema"],1);integer(data["capacity"],1);integer(data["session_drop_ns"],0)
    if data["vmhwm_kib"] is not None:integer(data["vmhwm_kib"],1)
    assert data["schema"]==17 and data["production_identity"]==identity["variants"][row["variant"]]["identity"]
    assert data["counting"] is False and data["diagnostic"] is False and data["complete"] is True
    assert data["mode"]==row["mode"] and data["capacity"]==case["capacity"]
    assert data["vmhwm_kib"]==row["vmhwm_kib"] and data["session_drop_ns"]==row["session_drop_ns"]
    assert equal_json([record_metadata(r) for r in data["records"]],row["records"]),"report metric/observation mismatch"
    references=protocol["reference_observations"][f"{row['case']}:{row['mode']}"]
    assert len(data["records"])==len(references)==4
    for r,reference in zip(data["records"],references):
        assert r["scenario"]==reference["scenario"] and semantic_digest(r)==reference["semantic_sha256"]
        assert equal_json(r["raw_snapshots"],[])
        if "llvm_sha256" in reference:
            assert sha(r["report"]["artifacts"]["llvm"])==reference["llvm_sha256"]
    prefix=folder.parent/(folder.name+"-runner")
    command=json.loads(prefix.with_suffix(".command.json").read_text())
    assert type(command["exit"]) is int and command["exit"]==0 and "outcome" not in command
    assert command["elapsed_ns"]==row["process_wall_ns"] and type(command["elapsed_ns"]) is int
    assert command["cwd"]==str(out)
    assert command["argv"]==list(map(str,[out/"bin"/f"{row['variant']}-timed",folder,row["mode"],out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]))
    assert prefix.with_suffix(".stderr").read_bytes()==b""
    assert prefix.with_suffix(".stdout").read_text()==f"pipeline107 {row['mode']} {folder} complete=true\n"


def schedule(cases):
    rng=random.Random(107)
    cells=[(case["name"],mode) for case in cases for mode in case["modes"]]
    rng.shuffle(cells)
    rows=[]
    for case,mode in cells:
        rows.append(dict(case=case,mode=mode,block=-1,warmup=True,order=["BASE","ON","ON","BASE"]))
        flips=[False]*32+[True]*32;rng.shuffle(flips)
        for block,reverse in enumerate(flips):
            rows.append(dict(case=case,mode=mode,block=block,warmup=False,
                             order=["ON","BASE","BASE","ON"] if reverse else ["BASE","ON","ON","BASE"]))
    assert len(rows)==1235 and sum(len(r["order"]) for r in rows)==4940
    return rows


def freeze(args):
    out=args.out.resolve();identity=prepared.verify(out)
    assert not (out/"confirmation-protocol.json").exists()
    cal=args.calibration.resolve();calibration=json.loads((cal/"summary.json").read_text())
    assert sha(cal/"samples.json")==CALIBRATION_SHA==calibration["samples_sha256"]
    assert calibration["status"]=="Pass" and calibration["production_admission"]=="NotRun"
    prepared.exact_commit(Path(__file__).resolve().parents[3], args.controller)
    import subprocess
    committed=subprocess.check_output(["git", "show", args.controller+":tools/research/107/confirm.py"], cwd=Path(__file__).resolve().parents[3])
    assert committed==Path(__file__).read_bytes(), "controller is not exact committed source"
    references={}
    controls=json.loads((out/"functional-controls.json").read_text())
    for case in identity["cases"]:
        for mode in case["modes"]:
            data=next(c for c in controls if (c["case"],c["mode"])==(case["name"],mode))["controls"]["BASE"]
            refs=[]
            for control in data:
                row_fields={"scenario","active_queries","cached_queries","sources"}
                normalized={k:control[k] for k in row_fields}
                normalized["report"]={k:v for k,v in control.items() if k not in row_fields|{"artifacts","native"}}
                digest=hashlib.sha256(json.dumps(normalized,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()
                ref=dict(scenario=control["scenario"],semantic_sha256=digest)
                if mode=="build":ref["llvm_sha256"]=control["artifacts"]["llvm"]
                refs.append(ref)
            references[f"{case['name']}:{mode}"]=refs
    write(out/"confirmation-schedule.json",schedule(identity["cases"]))
    write(out/"confirmation-protocol.json",dict(domain="ZIL-107-C1-CONFIRMATION-1",protocol_version=2,controller_commit=args.controller,controller_sha256=sha(__file__),
        build_identity_sha256=sha(out/"identity.json"),build_harness=identity["harness"],variants=identity["variants"],
        calibration_sha256=CALIBRATION_SHA,calibration_summary_sha256=sha(cal/"summary.json"),
        selection="C1 same-case campaign; independent A/A showed lower typical block variability, still insufficient to assert 1% resolution",
        layout="case campaign; two warm pairs then64 ABBA/BAAB blocks;32/32balanced",blocks=BLOCKS,
        total_processes=4940,requests=19760,warmup_processes=76,retained_processes=4864,
        schedule_sha256=sha(out/"confirmation-schedule.json"),family=FAMILY,alpha=ALPHA,margin=MARGIN,
        statistic="median of64 within-block ON/BASE ratios; each arm cost is arithmetic mean of its two complete observations",
        interval="exact binomial order18/47 with95-family Bonferroni",production_admission="NotRun",reference_observations=references,
        reference_origin="BASE plain complete functional controls, matched to count full stage bytes and prefilled native expected outputs",
        functional_sha256=sha(out/"functional.json"),functional_controls_sha256=sha(out/"functional-controls.json"),
        contract_sha256=sha(Path(__file__).resolve().parents[3]/"docs/research/107/CONFIRMATION.md"),
        primary=dict(case="modules512",mode="verify",endpoint="process"),off_timing="NotRun; function/work ablation only",
        runtime_helpers_sha256={name:sha(Path(__file__).with_name(name)) for name in prepared.SCRIPT_NAMES}))
    print("confirmation frozen: 4940 processes / 19760 requests / 95 endpoints",flush=True)


def verify(out):
    identity=prepared.verify(out);protocol=json.loads((out/"confirmation-protocol.json").read_text())
    assert sha(__file__)==protocol["controller_sha256"]
    assert sha(out/"identity.json")==protocol["build_identity_sha256"]
    assert sha(out/"confirmation-schedule.json")==protocol["schedule_sha256"]
    assert equal_json(json.loads((out/"confirmation-schedule.json").read_text()),schedule(identity["cases"]))
    assert protocol["calibration_sha256"]==CALIBRATION_SHA
    assert protocol["domain"]=="ZIL-107-C1-CONFIRMATION-1"
    assert type(protocol["protocol_version"]) is int and protocol["protocol_version"]==2
    assert equal_json(protocol["variants"],identity["variants"])
    for key,value in dict(blocks=64,total_processes=4940,requests=19760,warmup_processes=76,retained_processes=4864,family=95,alpha=.05,margin=1.01).items():
        assert type(protocol[key]) is type(value) and protocol[key]==value
    for name,digest in protocol["runtime_helpers_sha256"].items():
        assert sha(Path(__file__).with_name(name))==digest
    assert sha(out/"functional.json")==protocol["functional_sha256"]
    assert sha(out/"functional-controls.json")==protocol["functional_controls_sha256"]
    assert sha(Path(__file__).resolve().parents[3]/"docs/research/107/CONFIRMATION.md")==protocol["contract_sha256"]
    assert protocol["primary"]==dict(case="modules512",mode="verify",endpoint="process")
    return identity,protocol


def sample(args):
    out=args.out.resolve();identity,protocol=verify(out)
    assert sorted(os.sched_getaffinity(0))==[2], "root-coordinated CPU2 window required before any process"
    assert not (out/"confirmation-start.json").exists() and not (out/"confirmation-samples.jsonl").exists()
    write(out/"confirmation-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),
        affinity=sorted(os.sched_getaffinity(0)),protocol_sha256=sha(out/"confirmation-protocol.json")))
    cases={c["name"]:c for c in identity["cases"]};rows=[];last=time.monotonic()
    with (out/"confirmation-samples.jsonl").open("x") as journal:
        for cell in json.loads((out/"confirmation-schedule.json").read_text()):
            expected=None;block_rows=[]
            block_name="warm" if cell["warmup"] else f"b{cell['block']:02}"
            for position,variant in enumerate(cell["order"]):
                tag=f"confirmation/{cell['case']}-{cell['mode']}/{block_name}/p{position}"
                data,command,control=prepared.request(out,variant,cases[cell["case"]],cell["mode"],tag)
                if expected is None:expected=control
                else:assert control==expected,(cell,position,"semantic observations changed")
                row=dict(case=cell["case"],mode=cell["mode"],block=cell["block"],warmup=cell["warmup"],variant=variant,position=position,
                    process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],session_drop_ns=data["session_drop_ns"],
                    report_path=str(out/tag/f"{cell['case']}-{cell['mode']}-{variant}"/"report.json.gz"),
                    records=[record_metadata(x) for x in data["records"]])
                # Retain successful prefixes even if another process fails.
                journal.write(json.dumps(row,ensure_ascii=False,sort_keys=True,separators=(",",":"))+"\n")
                journal.flush();rows.append(row);block_rows.append(row)
            assert len(block_rows)==4
            if time.monotonic()-last>=25:
                print("confirmation",len(rows),"/ 4940;",cell["case"],cell["mode"],block_name,flush=True);last=time.monotonic()
    assert len(rows)==4940 and sum(r["warmup"] for r in rows)==76
    write(out/"confirmation-samples.json",rows)
    write(out/"confirmation-finish.json",dict(status="Pass",scope="acquisition only; admission separate",window=args.window,
        utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),processes=4940,requests=19760,warmup_processes=76,retained_processes=4864,
        samples_sha256=sha(out/"confirmation-samples.json"),journal_sha256=sha(out/"confirmation-samples.jsonl"),protocol_sha256=sha(out/"confirmation-protocol.json")))
    print("confirmation complete; release exclusive window before analysis",flush=True)


def analyze(out):
    assert not (out/"confirmation-summary.json").exists(), "retain existing analysis"
    identity,protocol=verify(out)
    rows=load_sealed_rows(out)
    frozen_schedule=json.loads((out/"confirmation-schedule.json").read_text())
    validate_observations(rows,frozen_schedule,identity["cases"],out)
    for row in rows:validate_capture(out,row,identity,protocol)
    k=max(i for i in range(BLOCKS//2) if 2*sum(math.comb(BLOCKS,j) for j in range(i+1))/2**BLOCKS<=ALPHA/FAMILY)
    assert k==17
    cells=[];processes=[]
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        selected=[r for r in rows if not r["warmup"] and (r["case"],r["mode"])==(case,mode)]
        assert len(selected)==256
        scenarios=[r["scenario"] for r in selected[0]["records"]]
        for scenario in scenarios+["process"]:
            get=lambda r:r["process_wall_ns"] if scenario=="process" else next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario)
            blocks=[];means={v:[] for v in ARMS};raw_values={v:[] for v in ARMS}
            for block in range(BLOCKS):
                arms={v:sorted([r for r in selected if (r["block"],r["variant"])==(block,v)],key=lambda r:r["position"]) for v in ARMS}
                assert all(len(a)==2 for a in arms.values())
                values={v:[get(r) for r in arm] for v,arm in arms.items()}
                for v in ARMS:means[v].append(statistics.mean(values[v]));raw_values[v].extend(values[v])
                blocks.append(dict(block=block,values=values,ratio=sum(values["ON"])/sum(values["BASE"]),
                                   first=next(v for v,a in arms.items() if a[0]["position"]==0)))
            ratios=[r["ratio"] for r in blocks];ordered=sorted(ratios)
            row=dict(case=case,mode=mode,scenario=scenario,blocks=blocks,paired_ratios=ratios,
                paired_median=statistics.median(ratios),simultaneous_95=[ordered[k],ordered[BLOCKS-k-1]],
                arm_medians_single_observations={v:statistics.median(x) for v,x in raw_values.items()},
                arm_medians_block_means={v:statistics.median(x) for v,x in means.items()},
                ratio_by_first_arm={v:[b["ratio"] for b in blocks if b["first"]==v] for v in ARMS},
                ratio_by_time_half=[ratios[:32],ratios[32:]])
            assert all(len(x)==32 for x in row["ratio_by_first_arm"].values())
            if scenario=="process":
                row["vmhwm_kib"]={v:[r["vmhwm_kib"] for r in selected if r["variant"]==v] for v in ARMS};processes.append(row)
            else:cells.append(row)
    assert len(cells)==76 and len(processes)==19
    all_cells=cells+processes
    short=lambda r:{k:r[k] for k in ["case","mode","scenario","paired_median","simultaneous_95"]}
    primary=next(r for r in processes if (r["case"],r["mode"])==("modules512","verify"))
    regressions=[short(r) for r in all_cells if r["simultaneous_95"][0]>MARGIN]
    uncertain=[short(r) for r in all_cells if r["simultaneous_95"][1]>MARGIN]
    decision="NoGo" if regressions else ("IndependentReviewRequired" if not uncertain and primary["simultaneous_95"][1]<1 else "Inconclusive")
    write(out/"confirmation-summary.json",dict(status="Pass",scope="all95 registered endpoints computed; production decision separate",
        production_admission={"ON":decision},samples_sha256=sha(out/"confirmation-samples.json"),
        protocol_sha256=sha(out/"confirmation-protocol.json"),processes_sampled=4940,requests=19760,warmup_processes=76,retained_processes=4864,
        method=dict(blocks=64,observations_per_arm_per_block=2,family=95,k=17,order_statistics=[18,47],alpha=.05,margin=1.01,
            per_comparison_tail_probability=2*sum(math.comb(64,j) for j in range(18))/2**64),
        primary=short(primary),cells=cells,processes=processes,regressions=regressions,
        cannot_exclude_one_percent_regression=uncertain,point_estimates_above_one_percent=[short(r) for r in all_cells if r["paired_median"]>MARGIN],
        limitations=["A/A calibration retained measurable noise; nominal simultaneous coverage assumes stable sufficiently independent blocks",
            "No sample removal, sample expansion, endpoint removal or post-result margin/statistic change",
            "Plain production binary only; counted controls and stage probes do not supply timing admission",
            "Parent VmHWM excludes clang child; all raw process values reported",
            "Same-host bounded ZIL compiler result; OFF timing not measured; no Rust-product claim or attribution to independent101"] ))
    print("ON decision:",decision,"explicit regressions:",len(regressions),"upper bounds above1%:",len(uncertain),flush=True)


if __name__=="__main__":
    if not __debug__: raise SystemExit("107 assertions require ordinary Python; -O is forbidden")
    p=argparse.ArgumentParser();sub=p.add_subparsers(dest="operation",required=True)
    q=sub.add_parser("freeze");q.add_argument("--out",type=Path,required=True);q.add_argument("--calibration",type=Path,required=True);q.add_argument("--controller",required=True)
    q=sub.add_parser("sample");q.add_argument("--out",type=Path,required=True);q.add_argument("--window",required=True)
    for name in ["verify","analyze"]:
        q=sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args=p.parse_args()
    if args.operation=="freeze":freeze(args)
    elif args.operation=="sample":sample(args)
    else:globals()[args.operation](args.out.resolve())

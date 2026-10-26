#!/usr/bin/env python3
"""Final bounded independent A/A layout calibration; 660 original processes."""
from __future__ import annotations
import argparse, json, math, os, random, shutil, statistics, time
from pathlib import Path
from support import BASE, legacy, sha, write, inventory
from calibrate import launch, consume
import diagnose

BLOCKS = 8


def schedule(cases):
    rng=random.Random(10102)
    cells=[(c["name"],mode) for c in cases for mode in c["modes"]]
    rng.shuffle(cells)
    flips={}
    for policy in ["C0","C1"]:
        for case,mode in cells:
            x=[False]*4+[True]*4;rng.shuffle(x);flips[(policy,case,mode)]=x
    def row(policy,case,mode,block,warm=False):
        return dict(policy=policy,case=case,mode=mode,block=block,binary="BASE-timed",deferred=False,warmup=warm,
                    order=["W0"] if warm else (["A1","A0","A0","A1"] if flips[(policy,case,mode)][block] else ["A0","A1","A1","A0"]))
    result=[]
    for half in range(2):
        order=cells if half==0 else list(reversed(cells))
        for policy in (["C0","C1"] if half==0 else ["C1","C0"]):
            if policy=="C0":
                for block in range(half*4,half*4+4):
                    for case,mode in order:result.append(row(policy,case,mode,block))
            else:
                for case,mode in order:
                    result.append(row(policy,case,mode,half*4,True))
                    for block in range(half*4,half*4+4):result.append(row(policy,case,mode,block))
    assert len(result)==180 and sum(len(r["order"]) for r in result)==660
    return result


def prepare(args):
    previous=args.previous.resolve();old=diagnose.verify(previous)
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    cases=legacy.inputs(args.repo.resolve(),out);assert cases==old["cases"]
    for case in cases:(out/"edit-manifests"/case["name"]/"scenarios.txt").write_text("\n".join(diagnose.SCENARIOS)+"\n")
    (out/"bin").mkdir();shutil.copy2(previous/"bin/BASE-timed",out/"bin/BASE-timed")
    assert sha(out/"bin/BASE-timed")==old["binaries"]["BASE-timed"]["sha256"]
    write(out/"schedule.json",schedule(cases))
    write(out/"identity.json",dict(base=BASE,harness=args.harness,cases=cases,previous=str(previous),
        previous_identity_sha256=sha(previous/"identity.json"),binary_sha256=sha(out/"bin/BASE-timed"),
        scripts={n:sha(Path(__file__).with_name(n)) for n in ["campaign.py","calibrate.py","diagnose.py","probe.py","support.py"]},
        input_sha256=inventory(out/"inputs"),edits_sha256=inventory(out/"edit-manifests"),schedule_sha256=sha(out/"schedule.json"),
        planned_processes=660,planned_requests=2640,warmup_processes=20,retained_processes=640,margin=1.01,
        purpose="last independent A/A calibration; candidate formal samples NotRun"))
    print("campaign calibration frozen: 660 processes / 2640 requests",flush=True)


def verify(out):
    identity=json.loads((out/"identity.json").read_text())
    assert sha(out/"bin/BASE-timed")==identity["binary_sha256"]
    for name,digest in identity["scripts"].items():assert sha(Path(__file__).with_name(name))==digest
    assert inventory(out/"inputs")==identity["input_sha256"] and inventory(out/"edit-manifests")==identity["edits_sha256"]
    assert sha(out/"schedule.json")==identity["schedule_sha256"]
    assert json.loads((out/"schedule.json").read_text())==schedule(identity["cases"])
    return identity


def sample(args):
    out=args.out.resolve();identity=verify(out);assert not (out/"sample-start.json").exists()
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),affinity=sorted(os.sched_getaffinity(0))))
    rows=[];cases={c["name"]:c for c in identity["cases"]};began=time.monotonic();last=began
    for cell in json.loads((out/"schedule.json").read_text()):
        case=cases[cell["case"]];source=out/"inputs"/case["edit"];before=sha(source);expected=None
        for position,label in enumerate(cell["order"]):
            position=4 if cell["warmup"] else position
            item=launch(out,case,cell,position,label)
            row,control=consume(out,item,case,cell,position,label);row["warmup"]=cell["warmup"];rows.append(row)
            if expected is None:expected=control
            else:assert control==expected,(cell,position)
        assert sha(source)==before
        write(out/"samples.json",rows)
        if time.monotonic()-last>=25:
            print("campaign acquisition",len(rows),"/ 660",flush=True);last=time.monotonic()
    assert len(rows)==660 and sum(r["warmup"] for r in rows)==20
    write(out/"sample-finish.json",dict(status="Pass",scope="independent A/A acquisition only",window=args.window,
        utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),processes=660,requests=2640,
        warmup_processes=20,retained_processes=640,samples_sha256=sha(out/"samples.json")))
    print("campaign acquisition complete; release window before analysis",flush=True)


def analyze(out):
    rows=json.loads((out/"samples.json").read_text());assert len(rows)==660 and sum(r["warmup"] for r in rows)==20
    cells=[]
    for policy in ["C0","C1"]:
        for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
            rr=[r for r in rows if not r["warmup"] and (r["policy"],r["case"],r["mode"])==(policy,case,mode)]
            assert len(rr)==32
            for scenario in diagnose.SCENARIOS+["process"]:
                get=lambda r:r["process_wall_ns"] if scenario=="process" else next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario)
                blocks=[]
                for b in range(BLOCKS):
                    values={label:[get(r) for r in rr if (r["block"],r["label"])==(b,label)] for label in ["A0","A1"]}
                    assert all(len(v)==2 for v in values.values())
                    blocks.append(dict(block=b,values=values,ratio=statistics.mean(values["A1"])/statistics.mean(values["A0"])))
                aggregate={}
                for n in [1,2,4,8]:
                    ratios=[]
                    for start in range(0,BLOCKS,n):
                        arms={label:[v for b in blocks[start:start+n] for v in b["values"][label]] for label in ["A0","A1"]}
                        ratios.append(statistics.mean(arms["A1"])/statistics.mean(arms["A0"]))
                    aggregate[str(n)]=dict(ratios=ratios,median=statistics.median(ratios))
                cells.append(dict(policy=policy,case=case,mode=mode,scenario=scenario,blocks=blocks,aggregation=aggregate))
    write(out/"summary.json",dict(status="Pass",scope="last independent A/A calibration; no production admission",production_admission="NotRun",
        samples_sha256=sha(out/"samples.json"),cells=cells,
        policy_summaries={p:{str(n):dict(median_absolute_log=statistics.median(abs(math.log(x)) for r in cells if r["policy"]==p for x in r["aggregation"][str(n)]["ratios"]),
                endpoint_medians_outside_one_percent=sum(abs(r["aggregation"][str(n)]["median"]-1)>.01 for r in cells if r["policy"]==p)) for n in [1,2,4,8]} for p in ["C0","C1"]},
        limitations=["Eight blocks cannot prove simultaneous 1% non-inferiority; no more calibration samples are authorized by this contract",
            "Every raw process retained, including twenty predeclared warm processes; no removal based on timing",
            "Warm state plus same-case campaigns is a combined layout treatment; it does not separately identify each physical cause"]))


if __name__=="__main__":
    p=argparse.ArgumentParser();sub=p.add_subparsers(dest="operation",required=True)
    q=sub.add_parser("prepare");q.add_argument("--repo",type=Path,required=True);q.add_argument("--previous",type=Path,required=True);q.add_argument("--harness",required=True);q.add_argument("--out",type=Path,required=True)
    q=sub.add_parser("sample");q.add_argument("--out",type=Path,required=True);q.add_argument("--window",required=True)
    for name in ["verify","analyze"]:
        q=sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args=p.parse_args()
    if args.operation=="prepare":prepare(args)
    elif args.operation=="sample":sample(args)
    else:globals()[args.operation](args.out.resolve())

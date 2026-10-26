#!/usr/bin/env python3
"""Independent same-SHA A/A measurement controls, frozen before candidate data."""
from __future__ import annotations
import argparse, hashlib, json, math, os, random, resource, shutil, statistics, subprocess, time
from pathlib import Path
from support import BASE, ENV, legacy, sha, write, inventory, normalize, compact_report
from probe import TIMES, NAMES
import diagnose

BLOCKS = 8
POLICIES = [("P0", "BASE-timed", False), ("P1", "BASE-timed", True), ("D0", "BASE-timed-diag", True)]
FIELDS = ["ru_utime", "ru_stime", "ru_minflt", "ru_majflt", "ru_inblock", "ru_oublock", "ru_nvcsw", "ru_nivcsw"]


def schedule(cases):
    rng = random.Random(10101)
    cells = [(c["name"],mode) for c in cases for mode in c["modes"]]
    orders = {}
    for policy,_,_ in POLICIES:
        for case,mode in cells:
            flips = [False]*4 + [True]*4
            rng.shuffle(flips)
            orders[(policy,case,mode)] = flips
    rows=[]
    for block in range(BLOCKS):
        policies = POLICIES[block%3:] + POLICIES[:block%3]
        for policy,key,deferred in policies:
            for case,mode in cells:
                rows.append(dict(block=block,policy=policy,binary=key,deferred=deferred,case=case,mode=mode,
                                 order=["A1","A0","A0","A1"] if orders[(policy,case,mode)][block] else ["A0","A1","A1","A0"]))
    assert len(rows)==240
    return rows


def prepare(args):
    previous = args.previous.resolve()
    old = diagnose.verify(previous)
    out = args.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    cases = legacy.inputs(args.repo.resolve(),out)
    assert cases == old["cases"]
    for case in cases:
        (out/"edit-manifests"/case["name"]/"scenarios.txt").write_text("\n".join(diagnose.SCENARIOS)+"\n")
    (out/"bin").mkdir()
    binaries={}
    for key in ["BASE-timed","BASE-timed-diag"]:
        shutil.copy2(previous/"bin"/key,out/"bin"/key)
        assert sha(out/"bin"/key)==old["binaries"][key]["sha256"]
        binaries[key]=dict(sha256=sha(out/"bin"/key),source_identity=old["binaries"][key],
                           source_manifest_sha256=sha(previous/f"source-{key}-sha256.json"))
    write(out/"schedule.json",schedule(cases))
    write(out/"identity.json",dict(base=BASE,previous=str(previous),previous_identity_sha256=sha(previous/"identity.json"),
        script_commit=args.harness,cases=cases,policies=POLICIES,blocks=BLOCKS,binaries=binaries,
        scripts={name:sha(Path(__file__).with_name(name)) for name in ["calibrate.py","diagnose.py","probe.py","support.py"]},
        input_sha256=inventory(out/"inputs"),edits_sha256=inventory(out/"edit-manifests"),schedule_sha256=sha(out/"schedule.json"),
        functional_source="diag001 fifty complete differential processes, 200 requests and 60 native outputs; exact same binary bytes",
        formal_candidate_samples="NotRun",margin=1.01))
    print("calibration frozen: 240 blocks / 960 processes / 3840 requests",flush=True)


def verify(out):
    identity=json.loads((out/"identity.json").read_text())
    for key,spec in identity["binaries"].items():assert sha(out/"bin"/key)==spec["sha256"]
    for name,digest in identity["scripts"].items():assert sha(Path(__file__).with_name(name))==digest,name
    assert inventory(out/"inputs")==identity["input_sha256"]
    assert inventory(out/"edit-manifests")==identity["edits_sha256"]
    assert sha(out/"schedule.json")==identity["schedule_sha256"]
    assert json.loads((out/"schedule.json").read_text())==schedule(identity["cases"])
    return identity


def launch(out,case,cell,position,label):
    folder=out/"samples"/f"b{cell['block']:02}"/cell["policy"]/f"{case['name']}-{cell['mode']}-{position}-{label}"
    folder.parent.mkdir(parents=True,exist_ok=True)
    argv=list(map(str,[out/"bin"/cell["binary"],folder,cell["mode"],out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]))
    usage0=resource.getrusage(resource.RUSAGE_CHILDREN)
    began=time.perf_counter_ns()
    try:
        result=subprocess.run(argv,cwd=out,env=ENV,capture_output=True,timeout=180)
        ended=time.perf_counter_ns()
        code,stdout,stderr=result.returncode,result.stdout,result.stderr
    except subprocess.TimeoutExpired as error:
        ended=time.perf_counter_ns()
        code,stdout,stderr=None,error.stdout or b"",error.stderr or b""
    usage1=resource.getrusage(resource.RUSAGE_CHILDREN)
    usage={name:getattr(usage1,name)-getattr(usage0,name) for name in FIELDS}
    command=dict(argv=argv,cwd=str(out),exit=code,elapsed_ns=ended-began,launch_monotonic_ns=began,
                 rusage_children_delta=usage,cumulative_children_maxrss_kib=usage1.ru_maxrss)
    return dict(folder=folder,command=command,stdout=stdout,stderr=stderr)


def consume(out,item,case,cell,position,label):
    began=time.perf_counter_ns();cpu=time.process_time_ns()
    folder=item["folder"]
    prefix=folder.parent/(folder.name+"-runner")
    prefix.with_suffix(".stdout").write_bytes(item["stdout"])
    prefix.with_suffix(".stderr").write_bytes(item["stderr"])
    write(prefix.with_suffix(".command.json"),item["command"])
    assert item["command"]["exit"]==0, str(prefix)
    data=json.loads((folder/"report.json").read_text())
    assert data["production_identity"]==BASE and data["schema"]==17 and data["complete"] and not data["counting"]
    assert data["diagnostic"]==(cell["policy"]=="D0")
    assert [r["scenario"] for r in data["records"]]==diagnose.SCENARIOS
    controls=[]
    for r in data["records"]:
        assert r["report"]["verification_runs"]==1 and r["active_queries"]==0
        controls.append(normalize(r))
    compact_report(folder/"report.json")
    row=dict(block=cell["block"],policy=cell["policy"],case=case["name"],mode=cell["mode"],label=label,position=position,
        binary=cell["binary"],deferred=cell["deferred"],process_wall_ns=item["command"]["elapsed_ns"],
        launch_monotonic_ns=item["command"]["launch_monotonic_ns"],rusage_children_delta=item["command"]["rusage_children_delta"],
        vmhwm_kib=data["vmhwm_kib"],session_drop_ns=data["session_drop_ns"],report_path=str(folder/"report.json.gz"),
        records=[{k:r[k] for k in ["scenario","api_wall_ns","counts","phase_ns","phase_calls","observation_ns"]} for r in data["records"]],
        postprocess_wall_ns=time.perf_counter_ns()-began,postprocess_cpu_ns=time.process_time_ns()-cpu)
    return row,controls


def sample(args):
    out=args.out.resolve();identity=verify(out)
    assert not (out/"sample-start.json").exists()
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),affinity=sorted(os.sched_getaffinity(0))))
    cases={c["name"]:c for c in identity["cases"]}
    rows=[]
    for index,cell in enumerate(json.loads((out/"schedule.json").read_text())):
        case=cases[cell["case"]];source=out/"inputs"/case["edit"];before=sha(source)
        pending=[];expected=None
        for position,label in enumerate(cell["order"]):
            item=launch(out,case,cell,position,label)
            if item["command"]["exit"]!=0:
                # Preserve the failed process immediately and stop; a killed
                # editor must not seed subsequent measurements with its edits.
                consume(out,item,case,cell,position,label)
            if cell["deferred"]:
                pending.append((item,position,label))
            else:
                row,control=consume(out,item,case,cell,position,label);rows.append(row)
                if expected is None:expected=control
                else:assert control==expected,(cell,position)
        for item,position,label in pending:
            row,control=consume(out,item,case,cell,position,label);rows.append(row)
            if expected is None:expected=control
            else:assert control==expected,(cell,position)
        assert sha(source)==before
        write(out/"samples.json",rows)
        if (index+1)%30==0:print("calibration block round",cell["block"]+1,"/",BLOCKS,"complete",flush=True)
    assert len(rows)==960
    write(out/"sample-finish.json",dict(status="Pass",scope="independent A/A acquisition only",window=args.window,
        utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),processes=960,requests=3840,samples_sha256=sha(out/"samples.json")))
    print("calibration finished; release window before analyze",flush=True)


def analyze(out):
    rows=json.loads((out/"samples.json").read_text());assert len(rows)==960
    results=[]
    for policy,_,_ in POLICIES:
        for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
            selected=[r for r in rows if (r["policy"],r["case"],r["mode"])==(policy,case,mode)]
            assert len(selected)==32
            for scenario in diagnose.SCENARIOS+["process"]:
                get=lambda r:r["process_wall_ns"] if scenario=="process" else next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario)
                blocks=[]
                for block in range(BLOCKS):
                    arms={label:[r for r in selected if (r["block"],r["label"])==(block,label)] for label in ["A0","A1"]}
                    assert all(len(a)==2 for a in arms.values())
                    values={label:[get(r) for r in arm] for label,arm in arms.items()}
                    blocks.append(dict(block=block,values=values,ratio=statistics.mean(values["A1"])/statistics.mean(values["A0"])))
                aggregation={}
                for size in [1,2,4,8]:
                    ratios=[]
                    for start in range(0,BLOCKS,size):
                        arms={label:[x for b in blocks[start:start+size] for x in b["values"][label]] for label in ["A0","A1"]}
                        ratios.append(statistics.mean(arms["A1"])/statistics.mean(arms["A0"]))
                    aggregation[str(size)]=dict(ratios=ratios,median=statistics.median(ratios),max_absolute_log=max(abs(math.log(x)) for x in ratios))
                results.append(dict(policy=policy,case=case,mode=mode,scenario=scenario,blocks=blocks,aggregation=aggregation))
    write(out/"summary.json",dict(status="Pass",scope="independent A/A stability, not production admission",production_admission="NotRun",
        samples_sha256=sha(out/"samples.json"),cells=results,times=TIMES,counters=NAMES,
        policy_summaries={p:{str(n):dict(median_absolute_log=statistics.median(abs(math.log(x)) for r in results if r["policy"]==p for x in r["aggregation"][str(n)]["ratios"]),
                            endpoint_medians_outside_one_percent=sum(abs(r["aggregation"][str(n)]["median"]-1)>.01 for r in results if r["policy"]==p)) for n in [1,2,4,8]} for p,_,_ in POLICIES},
        limitations=["8 independent diagnostic blocks cannot prove 95 simultaneous 1% non-inferiority",
            "All 960 processes retained; no culling based on wall/CPU, switches, faults or IO",
            "RUSAGE_CHILDREN delta includes already-waited descendants; CPU and wall boundaries differ slightly",
            "Stage timers inclusive; D0 cannot substitute for unprobed production measurements"]))


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

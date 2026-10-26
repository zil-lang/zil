#!/usr/bin/env python3
"""Five registered diagnostic rounds; never a production admission decision."""
from __future__ import annotations
import argparse, json, os, platform, shutil, statistics, subprocess, time
from pathlib import Path
from probe import NAMES, TIMES, instrument, make_dual
from support import BASE, HISTORICAL, ENV, legacy, sha, write, inventory, invoke, extract, normalize, compact_report, archive_directory

ROUNDS = 5
SCENARIOS = ["cold", "unchanged", "private_edit", "public_edit"]
GROUPS = [
    ("separate", [("BASE", "BASE-timed-diag", 0), ("H", "H-diag", 0)]),
    ("same_sha", [("A0", "BASE-timed", 0), ("A1", "BASE-timed", 0)]),
    ("same_binary", [("BASE", "DUAL-diag", 0), ("H", "DUAL-diag", 1)]),
]

def request(out, binary_key, case, mode, tag, label, algorithm=0, native=False):
    identity=json.loads((out/"identity.json").read_text())
    spec=identity["binaries"][binary_key]
    folder=out/tag/f"{case['name']}-{mode}-{label}"
    source=out/"inputs"/case["edit"];before=sha(source)
    argv=[out/"bin"/binary_key,folder,mode,out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]
    command=invoke(argv,out,folder.parent/(folder.name+"-runner"),env={"ZIL101_ALGORITHM":str(algorithm)},timeout=180)
    data=json.loads((folder/"report.json").read_text())
    assert data["production_identity"]==spec["identity"] and data["schema"]==17 and data["complete"]
    assert data["diagnostic"]==spec["diagnostic"] and not data["counting"]
    assert sha(source)==before and [r["scenario"] for r in data["records"]]==SCENARIOS
    controls=[]
    for row in data["records"]:
        assert row["active_queries"]==0 and row["report"]["verification_runs"]==1
        control=normalize(row)
        if native and "executable" in row["report"]["artifacts"]:
            prefix=folder/(row["scenario"]+"-native")
            run=invoke([row["report"]["artifacts"]["executable"]],out,prefix,stdin=case["stdin"].encode(),timeout=20)
            assert run["exit"]==0 and prefix.with_suffix(".stdout").read_bytes()==case["stdout"].encode()
            assert prefix.with_suffix(".stderr").read_bytes()==b""
            control["native"]={"exit":run["exit"],"stdout_sha256":sha(prefix.with_suffix(".stdout"))}
        controls.append(control)
    write(folder/"control-hash.json",{"sha256":__import__("hashlib").sha256(json.dumps(controls,sort_keys=True,separators=(",",":")).encode()).hexdigest()})
    compact_report(folder/"report.json")
    return data,command,controls

def prepare(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False);repo=args.repo.resolve()
    cases=legacy.inputs(repo,out)
    for case in cases:
        (out/"edit-manifests"/case["name"]/"scenarios.txt").write_text("\n".join(SCENARIOS)+"\n")
    specs={
        "BASE-timed":dict(commit=BASE,identity=BASE,diagnostic=False,dual=False),
        "BASE-timed-diag":dict(commit=BASE,identity=BASE,diagnostic=True,dual=False),
        "H-diag":dict(commit=HISTORICAL,identity=HISTORICAL,diagnostic=True,dual=False),
        "DUAL-diag":dict(commit=HISTORICAL,identity=f"dual:{BASE}:{HISTORICAL}",diagnostic=True,dual=True),
    }
    write(out/"preregistered-inputs.json",dict(scope="diagnostic only",cases=cases,groups=GROUPS,rounds=ROUNDS,specs=specs,
        input_sha256=inventory(out/"inputs"),edits_sha256=inventory(out/"edit-manifests"),harness=args.harness,formal_admission="NotRun"))
    old_source=subprocess.check_output(["git","show",BASE+":crates/zil-types/src/resource_effects.rs"],cwd=repo,text=True)
    for key,spec in specs.items():
        tree=out/("source-"+key);tree.mkdir()
        extract(repo,spec["commit"],["Cargo.toml","Cargo.lock","crates"],tree)
        extract(repo,args.harness,["tools/research/101","tools/research/100","tools/research/089"],tree)
        if spec["dual"]:make_dual(tree,old_source)
        if spec["diagnostic"]:instrument(tree,diagnostic=True)
        write(out/("source-"+key+"-sha256.json"),inventory(tree))
        target=out/("target-"+key)
        flags="--check-cfg=cfg(zil101_count) --check-cfg=cfg(zil101_diag)"+(" --cfg zil101_diag" if spec["diagnostic"] else "")
        env={"ZIL101_PRODUCTION_IDENTITY":spec["identity"],"ZIL089_PRODUCTION_COMMIT":spec["identity"],"RUSTFLAGS":flags}
        invoke(["cargo","build","--offline","--locked","--release","--manifest-path",tree/"tools/research/101/Cargo.toml","--target-dir",target],tree,out/f"logs/build-{key}",env)
        dest=out/"bin"/key;dest.parent.mkdir(exist_ok=True);shutil.copy2(target/"release/pipeline101",dest)
        spec["sha256"]=sha(dest)
        write(out/f"logs/prune-{key}.json",dict(path=str(target),binary=str(dest),sha256=sha(dest),scope="reproducible Cargo target only"))
        shutil.rmtree(target)
        print("built",key,flush=True)
    identity=dict(scope="101 bounded attribution; not admission",base=BASE,historical=HISTORICAL,harness=args.harness,
        cases=cases,groups=GROUPS,rounds=ROUNDS,schema=17,binaries=specs,input_sha256=inventory(out/"inputs"),
        edits_sha256=inventory(out/"edit-manifests"),scripts={name:sha(Path(__file__).with_name(name)) for name in ["diagnose.py","probe.py","support.py","main.rs"]},
        counters=NAMES,times=TIMES,time_accounting="inclusive: load_sources includes discovery_parse; types_execute includes types_check_program includes types_effect; analyze_execute includes analyze; compile_path includes all API work; do not sum nested spans",
        rustc=subprocess.check_output(["rustc","-Vv"],text=True),cargo=subprocess.check_output(["cargo","-V"],text=True),clang=subprocess.check_output([ENV.get("ZIL_CLANG","clang"),"--version"],text=True),host=platform.platform())
    write(out/"identity.json",identity)
    functional(out)

def functional(out):
    identity=json.loads((out/"identity.json").read_text());cells=[]
    variants=[("BASE-timed",0),("BASE-timed-diag",0),("H-diag",0),("DUAL-diag",0),("DUAL-diag",1)]
    for case in identity["cases"]:
        for mode in case["modes"]:
            expected=None
            for key,algorithm in variants:
                label=f"{key}-{algorithm}"
                _,_,controls=request(out,key,case,mode+"-emit","functional",label,algorithm,native=True)
                if expected is None:expected=controls
                else:assert controls==expected,(case["name"],mode,key,algorithm,"diagnostic changed observations")
                archive_directory(out/"functional"/f"{case['name']}-{mode}-emit-{label}")
            cells.append(dict(case=case["name"],mode=mode,status="Pass"))
            write(out/"functional.json",dict(status="Pass" if len(cells)==10 else "Running",scope="diagnostic instrument and shared-binary equivalence",cells=cells,
                processes=len(cells)*5,requests=len(cells)*20,native_executions=60 if len(cells)==10 else None))
            print("diagnostic controls",case["name"],mode,"Pass",flush=True)

def verify(out):
    identity=json.loads((out/"identity.json").read_text())
    for name,digest in identity["scripts"].items():assert sha(Path(__file__).with_name(name))==digest,name
    for key,spec in identity["binaries"].items():
        assert sha(out/"bin"/key)==spec["sha256"]
        assert inventory(out/("source-"+key))==json.loads((out/("source-"+key+"-sha256.json")).read_text())
    assert inventory(out/"inputs")==identity["input_sha256"] and inventory(out/"edit-manifests")==identity["edits_sha256"]
    assert json.loads((out/"functional.json").read_text())["status"]=="Pass"
    return identity

def sample(args):
    out=args.out.resolve();identity=verify(out);assert not (out/"samples.json").exists()
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),cpu_affinity=sorted(os.sched_getaffinity(0)),scope="fixed 5-round attribution only"))
    rows=[]
    cells=[(case,mode) for case in identity["cases"] for mode in case["modes"]]
    for r in range(ROUNDS):
        for g,(group,variants) in enumerate(GROUPS):
            for cellno,(case,mode) in enumerate(cells):
                order=variants if (r+g+cellno)%2==0 else list(reversed(variants))
                expected=None
                for position,(label,key,algorithm) in enumerate(order):
                    data,command,controls=request(out,key,case,mode,f"samples/r{r:02}/{group}",label,algorithm)
                    if expected is None:expected=controls
                    else:assert controls==expected,(r,group,case["name"],mode)
                    rows.append(dict(round=r,group=group,case=case["name"],mode=mode,label=label,binary=key,algorithm=algorithm,position=position,
                        process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],session_drop_ns=data["session_drop_ns"],
                        report_path=str(out/f"samples/r{r:02}/{group}"/f"{case['name']}-{mode}-{label}"/"report.json.gz"),
                        records=[{k:x[k] for k in ["scenario","api_wall_ns","counts","phase_ns","phase_calls","observation_ns"]} for x in data["records"]]))
                    write(out/"samples.json",rows)
        print("diagnostic round",r+1,"/",ROUNDS,"complete",flush=True)
    assert len(rows)==300
    write(out/"sample-finish.json",dict(status="Pass",scope="complete diagnostic acquisition only",window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),processes=300,requests=1200,samples_sha256=sha(out/"samples.json")))
    analyze(out)

def analyze(out):
    rows=json.loads((out/"samples.json").read_text());assert len(rows)==300
    results=[]
    for group,variants in GROUPS:
        labels=[x[0] for x in variants]
        for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
            arms={label:sorted([r for r in rows if (r["group"],r["case"],r["mode"],r["label"])==(group,case,mode,label)],key=lambda r:r["round"]) for label in labels}
            assert all(len(x)==ROUNDS for x in arms.values())
            for scenario in SCENARIOS+["process"]:
                values={label:[r["process_wall_ns"] if scenario=="process" else next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario) for r in arm] for label,arm in arms.items()}
                ratios=[b/a for a,b in zip(values[labels[0]],values[labels[1]])]
                row=dict(group=group,case=case,mode=mode,scenario=scenario,values=values,paired_ratios=ratios,paired_median=statistics.median(ratios),phase_values={})
                if scenario!="process":
                    for label,arm in arms.items():
                        observed=[next(x for x in r["records"] if x["scenario"]==scenario) for r in arm]
                        row["phase_values"][label]={name:[x["phase_ns"][i] for x in observed] for i,name in enumerate(TIMES)}
                results.append(row)
    aa=[r for r in results if r["group"]=="same_sha"]
    write(out/"summary.json",dict(status="Pass",scope="all diagnostic rows retained; no production admission",production_admission="NotRun",cells=results,
        aa_medians_outside_one_percent=[{k:r[k] for k in ["case","mode","scenario","paired_median"]} for r in aa if abs(r["paired_median"]-1)>.01],
        limitations=["Five diagnostic rounds cannot establish 95-cell simultaneous 1% non-inferiority","Inclusive stage timers are nested, not additive","Same-binary switch separates algorithm choice from whole-binary differences but is instrumented","Old100 NoGo remains unchanged"]))
    print("diagnostic summary ready; A/A medians outside 1%:",sum(abs(r["paired_median"]-1)>.01 for r in aa),"/",len(aa),flush=True)

if __name__=="__main__":
    parser=argparse.ArgumentParser();sub=parser.add_subparsers(dest="operation",required=True)
    q=sub.add_parser("prepare");q.add_argument("--repo",type=Path,required=True);q.add_argument("--harness",required=True);q.add_argument("--out",type=Path,required=True)
    q=sub.add_parser("sample");q.add_argument("--out",type=Path,required=True);q.add_argument("--window",required=True)
    for name in ["functional","verify","analyze"]:
        q=sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    if args.operation=="prepare":prepare(args)
    elif args.operation=="sample":sample(args)
    else:globals()[args.operation](args.out.resolve())

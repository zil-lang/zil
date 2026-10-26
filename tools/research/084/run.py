#!/usr/bin/env python3
"""Freeze both production revisions; compare complete behavior before sampling."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tarfile
import time

BASE = "f95cdd940ca4c41490738c75144c6538c27c295c"
SCENARIOS = ["cold", "unchanged", "private_edit", "public_edit"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1",
           CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_DEV_DEBUG="0")
for key in ["CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
    ENV.pop(key, None)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write(path, value):
    path=Path(path); path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2,sort_keys=True)+"\n")

def invoke(command, cwd, prefix, env=None, timeout=300, required=True):
    prefix=Path(prefix); prefix.parent.mkdir(parents=True,exist_ok=True)
    start=time.perf_counter_ns()
    try:
        result=subprocess.run(list(map(str,command)),cwd=cwd,env=ENV|(env or {}),capture_output=True,timeout=timeout)
        record=dict(argv=list(map(str,command)),cwd=str(cwd),exit=result.returncode,elapsed_ns=time.perf_counter_ns()-start)
        stdout,stderr=result.stdout,result.stderr
    except subprocess.TimeoutExpired as error:
        record=dict(argv=list(map(str,command)),cwd=str(cwd),exit=None,outcome="Timeout",elapsed_ns=time.perf_counter_ns()-start)
        stdout,stderr=error.stdout or b"",error.stderr or b""
    prefix.with_suffix(".stdout").write_bytes(stdout)
    prefix.with_suffix(".stderr").write_bytes(stderr)
    write(prefix.with_suffix(".command.json"),record)
    if required and record["exit"]!=0:
        raise RuntimeError(f"command failed; retained {prefix}: {stderr[-1800:]!r}")
    return record

def extract(repo, commit, paths, destination):
    raw=subprocess.check_output(["git","archive","--format=tar",commit,*paths],cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive.getmembers():
            p=Path(member.name)
            if p.is_absolute() or ".." in p.parts or not (member.isfile() or member.isdir()):
                raise RuntimeError(f"unsafe source archive {member.name}")
        archive.extractall(destination,filter="data")

def inventory(root):
    return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob("*")) if p.is_file()}

def normalized(record):
    report=record["report"]
    # Keep all semantic report fields. Only duration, concrete output locations
    # and tool argv with those locations are omitted from bytewise comparison.
    result={k:v for k,v in report.items() if k not in ["seconds","artifacts","commands"]}
    result["artifacts"]={k:sha(path) for k,path in report["artifacts"].items() if k!="executable"}
    result.update({k:v for k,v in record.items() if k not in ["report","api_wall_ns"]})
    return result

def controls(directory, data, case, native):
    rows=[]
    for record in data["records"]:
        report=record["report"]
        row=normalized(record)
        if report["status"]=="Success":
            assert report["verification_runs"]==1 and record["active_queries"]==0
            if native and "executable" in report["artifacts"]:
                result=subprocess.run([report["artifacts"]["executable"]],input=case["stdin"].encode(),capture_output=True,timeout=15,env=ENV)
                observed=dict(exit=result.returncode,stdout=result.stdout.decode(),stderr=result.stderr.decode())
                prefix=directory/(record["scenario"]+"-native")
                prefix.with_suffix(".stdout").write_bytes(result.stdout)
                prefix.with_suffix(".stderr").write_bytes(result.stderr)
                write(prefix.with_suffix(".json"),dict(argv=[report["artifacts"]["executable"]],stdin=case["stdin"],**observed))
                assert observed==case["expected"][record["scenario"]],(case["name"],record["scenario"],observed)
                row["native"]=observed
        rows.append(row)
    write(directory/"controls.json",rows)
    return rows

def check_edit_shape(data, case):
    rows={r["scenario"]:r for r in data["records"]}
    assert rows["cold"]["report"]["input_fingerprint"]==rows["unchanged"]["report"]["input_fingerprint"]
    assert rows["cold"]["report"]["input_fingerprint"]!=rows["private_edit"]["report"]["input_fingerprint"]
    assert rows["private_edit"]["report"]["input_fingerprint"]!=rows["public_edit"]["report"]["input_fingerprint"]
    signatures=lambda name:[i["signature_fingerprint"] for i in rows[name]["report"]["scalar_interfaces"]]
    assert signatures("cold")==signatures("private_edit"),(case["name"],"private edit unexpectedly changed public signature")
    assert signatures("private_edit")!=signatures("public_edit"),(case["name"],"public edit did not change signature")
    reused=sum(e["disposition"]=="reused" for e in rows["unchanged"]["report"]["query_events"])
    if case["name"].endswith("default-capacity"):
        assert reused==0, "default capacity eviction was not exercised"
    else:
        assert reused>0, "unchanged request did not reuse raw queries"

def request(out, variant, case, mode, tag, native=False):
    identity=json.loads((out/"identity.json").read_text())
    directory=out/tag/f"{case['name']}-{mode}-{variant}"
    binary=out/f"target-{variant}/release/pipeline084"
    command=[binary,directory,mode,out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]
    touched={edit["target"] for edits in case["edits"].values() for edit in edits}
    before={path:sha(out/"inputs"/path) for path in touched}
    directory.parent.mkdir(parents=True,exist_ok=True)
    invocation=invoke(command,out,directory.parent/(directory.name+"-runner"),required=False,timeout=180)
    assert (directory/"report.json").exists(),f"no actual report: {directory}"
    data=json.loads((directory/"report.json").read_text())
    assert data["production_commit"]==identity[variant+"_commit"] and data["schema"]==14
    assert data["capacity"]==case["capacity"]
    assert before=={path:sha(out/"inputs"/path) for path in touched},"source edits were not restored"
    assert [r["scenario"] for r in data["records"]]==SCENARIOS
    assert bool(data["complete"])==(invocation["exit"]==0)
    result=controls(directory,data,case,native)
    if data["complete"]: check_edit_shape(data,case)
    return data,invocation,result

def prepare(args):
    out=args.out.resolve(); out.mkdir(parents=True,exist_ok=False)
    repo=args.repo.resolve()
    for commit in [BASE,args.candidate]:
        assert subprocess.check_output(["git","rev-parse","--verify",commit+"^{commit}"],cwd=repo,text=True).strip()==commit
    for variant,commit in [("baseline",BASE),("candidate",args.candidate)]:
        source=out/f"source-{variant}"; source.mkdir()
        extract(repo,commit,["Cargo.toml","Cargo.lock","crates"],source)
        extract(repo,args.candidate,["tools/research/084"],source)
        write(out/f"source-{variant}-sha256.json",inventory(source))
        invoke(["cargo","build","--offline","--locked","--release","--manifest-path",source/"tools/research/084/Cargo.toml","--target-dir",out/f"target-{variant}"],
               source,out/f"logs/build-{variant}",env={"ZIL084_PRODUCTION_COMMIT":commit})
    inputs=out/"inputs"; inputs.mkdir()
    extract(repo,args.candidate,["tests/perf/impl084","examples/mainline"],inputs)
    cases=json.loads((inputs/"tests/perf/impl084/cases.json").read_text())
    for case in cases:
        directory=out/"edit-manifests"/case["name"]; directory.mkdir(parents=True)
        for scenario,edits in case["edits"].items():
            (directory/(scenario+".tsv")).write_text("".join(f"{inputs/e['target']}\t{inputs/e['replacement']}\n" for e in edits))
    identity=dict(baseline_commit=BASE,candidate_commit=args.candidate,schema=14,
                  runner_sha256=sha(out/"source-candidate/tools/research/084/main.rs"),
                  guards_sha256=sha(out/"source-candidate/tools/research/084/guards.rs"),controller_sha256=sha(__file__),
                  case_manifest_sha256=sha(inputs/"tests/perf/impl084/cases.json"),input_sha256=inventory(inputs),
                  binaries={v:sha(out/f"target-{v}/release/pipeline084") for v in ["baseline","candidate"]},
                  environment={k:ENV[k] for k in ["CARGO_INCREMENTAL","CARGO_BUILD_JOBS","CARGO_PROFILE_RELEASE_DEBUG","CARGO_PROFILE_TEST_DEBUG","CARGO_PROFILE_DEV_DEBUG"]},
                  rustc=subprocess.check_output(["rustc","-Vv"],text=True),cargo=subprocess.check_output(["cargo","-V"],text=True),
                  clang=subprocess.check_output([ENV.get("ZIL_CLANG","clang"),"--version"],text=True),
                  profile="release codegen-units1 debug0 incremental0 jobs1",host=platform.platform(),warmup=1,rounds=7,cases=cases,
                  source_paths="identical frozen absolute paths; temporary source edits restored after every process",
                  cold="empty compiler session; OS file page cache not flushed",formal_samples="NotRun")
    write(out/"identity.json",identity)
    guard_results={}
    for variant in ["baseline","candidate"]:
        directory=out/f"guards-{variant}"
        invoke([out/f"target-{variant}/release/pipeline084","guards",directory],out,out/f"logs/guards-{variant}")
        data=json.loads((directory/"report.json").read_text())
        assert data["production_commit"]==identity[variant+"_commit"] and data["schema"]==14 and data["status"]=="Pass"
        guard_results[variant]=dict(analysis_n=data["analysis_n"],ct_n=data["ct_n"],requests=data["requests"],records=[normalized(r) for r in data["records"]])
    write(out/"guard-controls.json",guard_results)
    assert guard_results["baseline"]==guard_results["candidate"],"complete guard results differ"
    results=[]
    for case in cases:
        for mode in case["modes"]:
            pair={}
            for variant in ["baseline","candidate"]:
                data,command,control=request(out,variant,case,mode+"-emit","preflight",native=True)
                pair[variant]=dict(complete=data["complete"],control=control,exit=command["exit"])
            outcome="Complete" if all(v["complete"] for v in pair.values()) else "NonComplete"
            if outcome=="Complete": assert pair["baseline"]["control"]==pair["candidate"]["control"],(case["name"],mode,"complete outputs differ")
            results.append(dict(case=case["name"],mode=mode,outcome=outcome,variants=pair))
            write(out/"preflight.json",results)
            print(f"preflight {case['name']} {mode}: {outcome}",flush=True)
    eligible=[dict(case=r["case"],mode=r["mode"]) for r in results if r["outcome"]=="Complete"]
    write(out/"eligibility.json",eligible)
    print(json.dumps(dict(preflight_cells=len(results),complete=len(eligible),noncomplete=len(results)-len(eligible),guards=guard_results["baseline"]["requests"])))

def sample(args):
    out=args.out.resolve(); identity=json.loads((out/"identity.json").read_text())
    assert not (out/"samples.json").exists(),"never overwrite an earlier run"
    assert sha(__file__)==identity["controller_sha256"],"controller changed after preparation"
    eligible=json.loads((out/"eligibility.json").read_text())
    for variant in ["baseline","candidate"]:
        assert sha(out/f"target-{variant}/release/pipeline084")==identity["binaries"][variant]
        assert inventory(out/f"source-{variant}")==json.loads((out/f"source-{variant}-sha256.json").read_text())
    assert inventory(out/"inputs")==identity["input_sha256"]
    cases={case["name"]:case for case in identity["cases"]}; records=[]
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),eligible=eligible))
    for r in range(identity["warmup"]+identity["rounds"]):
        for cellno,cell in enumerate(eligible):
            case=cases[cell["case"]]
            variants=["baseline","candidate"] if (r+cellno)%2==0 else ["candidate","baseline"]
            pair={}
            for variant in variants:
                data,command,control=request(out,variant,case,cell["mode"],f"samples/r{r:02}")
                assert data["complete"]
                pair[variant]=control
                records.append(dict(round=r,warmup=r<identity["warmup"],case=case["name"],mode=cell["mode"],variant=variant,
                                    process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],records=data["records"]))
                write(out/"samples.json",records)
            assert pair["baseline"]==pair["candidate"],(cell,r,"same-input complete results differ")
        print(f"sampling round {r+1}/{identity['warmup']+identity['rounds']} complete",flush=True)
    write(out/"sample-finish.json",dict(status="Pass",window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),
                                      processes=len(records),requests=4*len(records),warmup_processes=sum(r["warmup"] for r in records),samples_sha256=sha(out/"samples.json")))
    analyze(out)

def interval(randomizer,values):
    draws=sorted(statistics.median(randomizer.choices(values,k=len(values))) for _ in range(2000))
    return [draws[49],draws[1949]]

def analyze(out):
    rows=json.loads((out/"samples.json").read_text()); cells=[]; processes=[]; randomizer=random.Random(84)
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        pairs={v:sorted([r for r in rows if not r["warmup"] and r["case"]==case and r["mode"]==mode and r["variant"]==v],key=lambda r:r["round"]) for v in ["baseline","candidate"]}
        for scenario in SCENARIOS:
            values={v:[next(x["api_wall_ns"] for x in row["records"] if x["scenario"]==scenario) for row in rr] for v,rr in pairs.items()}
            ratios=[c/b for b,c in zip(values["baseline"],values["candidate"])]
            cells.append(dict(case=case,mode=mode,scenario=scenario,api_ns=values,medians_ns={v:statistics.median(a) for v,a in values.items()},
                              paired_ratio=statistics.median(ratios),exploratory_95=interval(randomizer,ratios)))
        values={v:[r["process_wall_ns"] for r in rr] for v,rr in pairs.items()}
        ratios=[c/b for b,c in zip(values["baseline"],values["candidate"])]
        processes.append(dict(case=case,mode=mode,wall_ns=values,medians_ns={v:statistics.median(a) for v,a in values.items()},
                              paired_ratio=statistics.median(ratios),exploratory_95=interval(randomizer,ratios),
                              vmhwm_kib={v:[r["vmhwm_kib"] for r in rr] for v,rr in pairs.items()}))
    write(out/"summary.json",dict(status="Pass",sample_processes=len(rows),compiler_requests=4*len(rows),cells=cells,processes=processes,
          limitations=["single-host bounded result; no equivalent-Rust performance comparison",
                       "VmHWM covers runner and all four requests; excludes external clang child peak",
                       "process wall also includes source identity recording and report emission",
                       "cold means empty session, not flushed OS page cache",
                       "exploratory bootstrap intervals from seven pairs; no product-gate change"]))

if __name__=="__main__":
    parser=argparse.ArgumentParser(); commands=parser.add_subparsers(dest="command",required=True)
    p=commands.add_parser("prepare"); p.add_argument("--repo",type=Path,required=True); p.add_argument("--candidate",required=True); p.add_argument("--out",type=Path,required=True)
    p=commands.add_parser("sample"); p.add_argument("--out",type=Path,required=True); p.add_argument("--window",required=True)
    p=commands.add_parser("analyze"); p.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    if args.command=="prepare": prepare(args)
    elif args.command=="sample": sample(args)
    else: analyze(args.out.resolve())

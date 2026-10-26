#!/usr/bin/env python3
"""Frozen four-way complete requests; direct parse counts are separate builds."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import tarfile
import time
from instrument import instrument

VARIANTS = {
    "B": "2883f978de79ea0e8a09a4cb218fa68687f4f568",
    "K": "9b501ae9d6e6e1892695508c9c4cc110d09a6a77",
    "L": "62bbb97dcaa3879776875d7ac1fa2e8b6e834080",
    "KL": "351e6de6a2643f7e1bacd4554d3f0f258a3ad5a1",
}
SCENARIOS = ["cold", "unchanged", "private_edit", "public_edit"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1",
           CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_DEV_DEBUG="0")
for key in ["CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
    ENV.pop(key, None)

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path, value):
    path=Path(path); path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2,sort_keys=True)+"\n")
def inventory(root):
    return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob("*")) if p.is_file()}
def invoke(command,cwd,prefix,env=None,timeout=300,required=True):
    command=list(map(str,command)); prefix=Path(prefix); prefix.parent.mkdir(parents=True,exist_ok=True)
    start=time.perf_counter_ns()
    try:
        result=subprocess.run(command,cwd=cwd,env=ENV|(env or {}),capture_output=True,timeout=timeout)
        record=dict(argv=command,cwd=str(cwd),exit=result.returncode,elapsed_ns=time.perf_counter_ns()-start)
        stdout,stderr=result.stdout,result.stderr
    except subprocess.TimeoutExpired as e:
        record=dict(argv=command,cwd=str(cwd),exit=None,outcome="Timeout",elapsed_ns=time.perf_counter_ns()-start)
        stdout,stderr=e.stdout or b"",e.stderr or b""
    prefix.with_suffix(".stdout").write_bytes(stdout); prefix.with_suffix(".stderr").write_bytes(stderr)
    write(prefix.with_suffix(".command.json"),record)
    if required and record["exit"]!=0: raise RuntimeError(f"command failed; retained {prefix}: {stderr[-1500:]!r}")
    return record
def extract(repo,commit,paths,destination):
    raw=subprocess.check_output(["git","archive","--format=tar",commit,*paths],cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive.getmembers():
            p=Path(member.name)
            if p.is_absolute() or ".." in p.parts or not (member.isfile() or member.isdir()): raise RuntimeError(member.name)
        archive.extractall(destination,filter="data")

def normalized(record):
    report=record["report"]
    omitted=["seconds","artifacts","commands","query_events","cache_hit"]
    result={k:v for k,v in report.items() if k not in omitted}
    result["artifacts"]={k:sha(path) for k,path in report["artifacts"].items() if k!="executable"}
    result.update({k:v for k,v in record.items() if k not in ["report","api_wall_ns","cached_queries","lexer_calls","parser_calls"]})
    return result
def controls(directory,data,case,native):
    rows=[]
    for record in data["records"]:
        report=record["report"]; row=normalized(record)
        if report["status"]=="Success":
            assert report["verification_runs"]==1 and record["active_queries"]==0
            if native and "executable" in report["artifacts"]:
                result=subprocess.run([report["artifacts"]["executable"]],input=case["stdin"].encode(),capture_output=True,timeout=15,env=ENV)
                observed=dict(exit=result.returncode,stdout=result.stdout.decode(),stderr=result.stderr.decode())
                prefix=directory/(record["scenario"]+"-native")
                prefix.with_suffix(".stdout").write_bytes(result.stdout); prefix.with_suffix(".stderr").write_bytes(result.stderr)
                write(prefix.with_suffix(".json"),dict(argv=[report["artifacts"]["executable"]],stdin=case["stdin"],**observed))
                assert observed==case["expected"][record["scenario"]],(case["name"],record["scenario"],observed)
                row["native"]=observed
        rows.append(row)
    write(directory/"controls.json",rows); return rows
def check_shape(data,case):
    rows={r["scenario"]:r for r in data["records"]}
    assert rows["cold"]["report"]["input_fingerprint"]==rows["unchanged"]["report"]["input_fingerprint"]
    assert rows["cold"]["report"]["input_fingerprint"]!=rows["private_edit"]["report"]["input_fingerprint"]
    assert rows["private_edit"]["report"]["input_fingerprint"]!=rows["public_edit"]["report"]["input_fingerprint"]
    signatures=lambda name:[i["signature_fingerprint"] for i in rows[name]["report"]["scalar_interfaces"]]
    assert signatures("cold")==signatures("private_edit")
    assert signatures("private_edit")!=signatures("public_edit")
    reused=sum(e["disposition"]=="reused" for e in rows["unchanged"]["report"]["query_events"])
    assert (reused==0) if case["name"].endswith("default-capacity") else (reused>0)
def request(out,variant,case,mode,tag,counted=False,native=False):
    identity=json.loads((out/"identity.json").read_text())
    directory=out/tag/f"{case['name']}-{mode}-{variant}"
    binary=out/"bin"/f"{variant}-{'counted' if counted else 'timed'}"
    touched={e["target"] for edits in case["edits"].values() for e in edits}
    before={path:sha(out/"inputs"/path) for path in touched}
    directory.parent.mkdir(parents=True,exist_ok=True)
    command=[binary,directory,mode,out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]
    invocation=invoke(command,out,directory.parent/(directory.name+"-runner"),required=False,timeout=180)
    assert (directory/"report.json").exists(),directory
    data=json.loads((directory/"report.json").read_text())
    assert data["production_commit"]==identity["variants"][variant] and data["schema"]==15
    assert data["counting"]==counted and data["capacity"]==case["capacity"]
    assert before=={path:sha(out/"inputs"/path) for path in touched}
    assert [r["scenario"] for r in data["records"]]==SCENARIOS
    assert data["complete"] and invocation["exit"]==0,(case["name"],variant,invocation)
    check_shape(data,case)
    return data,invocation,controls(directory,data,case,native)

def prepare(args):
    out=args.out.resolve(); out.mkdir(parents=True,exist_ok=False); repo=args.repo.resolve()
    for name,commit in VARIANTS.items():
        assert subprocess.check_output(["git","rev-parse",commit+"^{commit}"],cwd=repo,text=True).strip()==commit
    for variant,commit in VARIANTS.items():
        source=out/f"source-{variant}"; source.mkdir()
        extract(repo,commit,["Cargo.toml","Cargo.lock","crates"],source)
        extract(repo,args.harness,["tools/research/089"],source)
        write(out/f"source-{variant}-sha256.json",inventory(source))
        counted=out/f"source-{variant}-counted"; shutil.copytree(source,counted); instrument(counted)
        write(out/f"source-{variant}-counted-sha256.json",inventory(counted))
        for kind,tree in [("timed",source),("counted",counted)]:
            target=out/f"target-{variant}-{kind}"
            env={"ZIL089_PRODUCTION_COMMIT":commit}
            if kind=="counted": env["RUSTFLAGS"]="--cfg zil089_count"
            invoke(["cargo","build","--offline","--locked","--release","--manifest-path",tree/"tools/research/089/Cargo.toml","--target-dir",target],tree,out/f"logs/build-{variant}-{kind}",env=env,timeout=600)
            destination=out/"bin"/f"{variant}-{kind}"; destination.parent.mkdir(exist_ok=True)
            shutil.copy2(target/"release/pipeline089",destination)
            assert sha(destination)==sha(target/"release/pipeline089")
            # Only this completed, reproducible build cache is removed. Source,
            # exact executable, command and logs remain as frozen raw evidence.
            write(out/f"logs/prune-{variant}-{kind}.json",dict(path=str(target),retained_binary=str(destination),sha256=sha(destination)))
            shutil.rmtree(target)
            print(f"prepared {variant} {kind}",flush=True)
    inputs=out/"inputs"; inputs.mkdir()
    extract(repo,VARIANTS["B"],["tests/perf/impl084","examples/mainline"],inputs)
    cases=json.loads((inputs/"tests/perf/impl084/cases.json").read_text())
    assert len(cases)==9 and sum(len(c["modes"]) for c in cases)==12
    for case in cases:
        directory=out/"edit-manifests"/case["name"]; directory.mkdir(parents=True)
        for scenario,edits in case["edits"].items():
            (directory/(scenario+".tsv")).write_text("".join(f"{inputs/e['target']}\t{inputs/e['replacement']}\n" for e in edits))
    identity=dict(variants=VARIANTS,harness_commit=args.harness,schema=15,controller_sha256=sha(__file__),instrument_sha256=sha(Path(__file__).with_name("instrument.py")),
        binaries={f"{v}-{kind}":sha(out/"bin"/f"{v}-{kind}") for v in VARIANTS for kind in ["timed","counted"]},
        input_sha256=inventory(inputs),case_manifest_sha256=sha(inputs/"tests/perf/impl084/cases.json"),
        environment={k:ENV[k] for k in ["CARGO_INCREMENTAL","CARGO_BUILD_JOBS","CARGO_PROFILE_RELEASE_DEBUG","CARGO_PROFILE_TEST_DEBUG","CARGO_PROFILE_DEV_DEBUG"]},
        rustc=subprocess.check_output(["rustc","-Vv"],text=True),cargo=subprocess.check_output(["cargo","-V"],text=True),
        clang=subprocess.check_output([ENV.get("ZIL_CLANG","clang"),"--version"],text=True),host=platform.platform(),
        profile="release codegen-units1 debug0 incremental0 jobs1",warmup=1,rounds=7,cases=cases,formal_samples="NotRun",
        counted_build="same source plus direct lexer/parser entry counters, --cfg zil089_count; excluded from timing",
        syntax_context="driver fixed default AttributeContext; no external deserialization or caller-supplied cache qualification")
    write(out/"identity.json",identity)
    functional(out)

def functional(out):
    identity=json.loads((out/"identity.json").read_text()); guards={}
    for variant in VARIANTS:
        directory=out/f"guards-{variant}"
        invoke([out/"bin"/f"{variant}-timed","guards",directory,out/"guard-common"],out,out/f"logs/guards-{variant}")
        data=json.loads((directory/"report.json").read_text())
        assert data["production_commit"]==VARIANTS[variant] and data["schema"]==15 and data["status"]=="Pass"
        guards[variant]=dict(analysis_n=data["analysis_n"],ct_n=data["ct_n"],requests=data["requests"],records=[normalized(r) for r in data["records"]])
        write(out/"guard-controls.json",guards)
        if variant!="B": assert guards[variant]==guards["B"],(variant,"guard semantic mismatch")
    rows=[]; counts=[]
    for case in identity["cases"]:
        for mode in case["modes"]:
            group={}
            for variant in VARIANTS:
                data,command,control=request(out,variant,case,mode+"-emit","preflight",native=True)
                group[variant]=control
                counted,_,count_control=request(out,variant,case,mode,"counts",counted=True)
                # Emit files intentionally absent in count requests; semantic
                # reports, actual inputs and interfaces must still match.
                without_artifacts=lambda seq:[{k:v for k,v in r.items() if k not in ["artifacts","native"]} for r in seq]
                assert without_artifacts(control)==without_artifacts(count_control),(case["name"],mode,variant,"counting changed behavior")
                actual=[]
                for r in counted["records"]:
                    assert r["lexer_calls"]==r["parser_calls"]
                    if variant in ["B","K"] or case["name"].endswith("default-capacity"):
                        assert r["parser_calls"]==len(r["sources"]),(variant,case["name"],r["scenario"])
                    elif r["scenario"]=="unchanged": assert r["parser_calls"]==0
                    actual.append(dict(scenario=r["scenario"],lexer_calls=r["lexer_calls"],parser_calls=r["parser_calls"],source_files=len(r["sources"]),query_events=r["report"]["query_events"]))
                counts.append(dict(case=case["name"],mode=mode,variant=variant,actual=actual))
                write(out/"parse-counts.json",counts)
            assert all(group[v]==group["B"] for v in VARIANTS),(case["name"],mode,"full emitted outputs differ")
            rows.append(dict(case=case["name"],mode=mode,status="Pass",variants=list(VARIANTS)))
            write(out/"functional.json",dict(status="Pass" if len(rows)==12 else "Running",cells=rows,guard_requests={k:v["requests"] for k,v in guards.items()},requests=len(rows)*32))
            print(f"functional {case['name']} {mode}: Pass",flush=True)

def verify_frozen(out):
    identity=json.loads((out/"identity.json").read_text())
    assert sha(__file__)==identity["controller_sha256"]
    assert sha(Path(__file__).with_name("instrument.py"))==identity["instrument_sha256"]
    for variant in VARIANTS:
        for kind in ["timed","counted"]:
            assert sha(out/"bin"/f"{variant}-{kind}")==identity["binaries"][f"{variant}-{kind}"]
            suffix="" if kind=="timed" else "-counted"
            assert inventory(out/f"source-{variant}{suffix}")==json.loads((out/f"source-{variant}{suffix}-sha256.json").read_text())
    assert inventory(out/"inputs")==identity["input_sha256"]
    assert json.loads((out/"functional.json").read_text())["status"]=="Pass"
    return identity
def sample(args):
    out=args.out.resolve(); identity=verify_frozen(out)
    assert not (out/"samples.json").exists(),"earlier sample cannot be overwritten"
    cells=[(c,m) for c in identity["cases"] for m in c["modes"]]
    schedule=[]
    variants=list(VARIANTS)
    for r in range(8):
        for cellno,(case,mode) in enumerate(cells):
            offset=(r+cellno)%4; order=variants[offset:]+variants[:offset]
            if (r//4)%2: order=list(reversed(order))
            schedule.append(dict(round=r,case=case["name"],mode=mode,order=order))
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),schedule=schedule))
    records=[]; cases={c["name"]:c for c in identity["cases"]}
    for cell in schedule:
        r=cell["round"]; case=cases[cell["case"]]; group={}
        for variant in cell["order"]:
            data,command,control=request(out,variant,case,cell["mode"],f"samples/r{r:02}")
            group[variant]=control
            records.append(dict(round=r,warmup=r==0,case=case["name"],mode=cell["mode"],variant=variant,
                process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],records=data["records"]))
            write(out/"samples.json",records)
        assert all(group[v]==group["B"] for v in VARIANTS),(cell,"sample outputs differ")
        print(f"sample r{r} {case['name']} {cell['mode']} done",flush=True)
    assert len(records)==384
    write(out/"sample-finish.json",dict(status="Pass",window=args.window,processes=len(records),requests=4*len(records),warmup_processes=48,samples_sha256=sha(out/"samples.json")))
    analyze(out)
def interval(rng,values):
    draws=sorted(statistics.median(rng.choices(values,k=len(values))) for _ in range(2000)); return [draws[49],draws[1949]]
def analyze(out):
    rows=json.loads((out/"samples.json").read_text()); cells=[]; processes=[]; rng=random.Random(89)
    assert len(rows)==384 and sum(not r["warmup"] for r in rows)==336
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        data={v:sorted([r for r in rows if not r["warmup"] and r["case"]==case and r["mode"]==mode and r["variant"]==v],key=lambda r:r["round"]) for v in VARIANTS}
        assert all(len(a)==7 for a in data.values())
        for variant in ["K","L","KL"]:
            for scenario in SCENARIOS:
                values={v:[next(x["api_wall_ns"] for x in row["records"] if x["scenario"]==scenario) for row in data[v]] for v in ["B",variant]}
                ratios=[c/b for b,c in zip(values["B"],values[variant])]
                cells.append(dict(case=case,mode=mode,variant=variant,scenario=scenario,api_ns=values,medians_ns={v:statistics.median(a) for v,a in values.items()},paired_ratio=statistics.median(ratios),exploratory_95=interval(rng,ratios)))
            values={v:[r["process_wall_ns"] for r in data[v]] for v in ["B",variant]}
            ratios=[c/b for b,c in zip(values["B"],values[variant])]
            processes.append(dict(case=case,mode=mode,variant=variant,wall_ns=values,medians_ns={v:statistics.median(a) for v,a in values.items()},paired_ratio=statistics.median(ratios),exploratory_95=interval(rng,ratios),vmhwm_kib={v:[r["vmhwm_kib"] for r in data[v]] for v in ["B",variant]}))
    write(out/"summary.json",dict(status="Pass",processes_sampled=len(rows),requests=4*len(rows),cells=cells,processes=processes,
        limitations=["single host; seven retained pairs; exploratory intervals; no Rust superiority claim","VmHWM excludes external clang child peak","process wall includes recording and session destruction","empty session cold; OS page cache not flushed","no product threshold or safety/Verify relaxation"]))

if __name__=="__main__":
    parser=argparse.ArgumentParser(); commands=parser.add_subparsers(dest="command",required=True)
    p=commands.add_parser("prepare"); p.add_argument("--repo",type=Path,required=True); p.add_argument("--harness",required=True); p.add_argument("--out",type=Path,required=True)
    p=commands.add_parser("sample"); p.add_argument("--out",type=Path,required=True); p.add_argument("--window",required=True)
    p=commands.add_parser("analyze"); p.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    if args.command=="prepare": prepare(args)
    elif args.command=="sample": sample(args)
    else: analyze(args.out.resolve())

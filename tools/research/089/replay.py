#!/usr/bin/env python3
"""Bounded cold functional reconstruction; never resamples historical timing."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time
from instrument import instrument
import run

HARNESS = "2bd4ed9f3bc7c3f9fdbb665bf047b803f19a6c7b"
SELECTED = ["B", "KL"]

def replay(repo, out):
    repo=repo.resolve();out=out.resolve();out.mkdir(parents=True,exist_ok=False)
    start=time.time(); checks=[]
    # The sampler and direct-entry patch have to be exactly the frozen tools.
    for name in ["run.py","instrument.py"]:
        expected=subprocess.check_output(["git","show",f"{HARNESS}:tools/research/089/{name}"],cwd=repo)
        assert Path(__file__).with_name(name).read_bytes()==expected,name
    for variant in SELECTED:
        commit=run.VARIANTS[variant]
        source=out/f"source-{variant}";source.mkdir()
        run.extract(repo,commit,["Cargo.toml","Cargo.lock","crates"],source)
        run.extract(repo,HARNESS,["tools/research/089"],source)
        counted=out/f"source-{variant}-counted";shutil.copytree(source,counted);instrument(counted)
        for kind,tree in [("plain",source),("counted",counted)]:
            run.write(out/f"source-{variant}-{kind}-sha256.json",run.inventory(tree))
            target=out/f"target-{variant}-{kind}"
            env={"ZIL089_PRODUCTION_COMMIT":commit}
            if kind=="counted":env["RUSTFLAGS"]="--cfg zil089_count"
            run.invoke(["cargo","build","--offline","--locked","--manifest-path",tree/"tools/research/089/Cargo.toml","--target-dir",target],tree,out/f"logs/build-{variant}-{kind}",env=env,timeout=600)
            binary=out/"bin"/f"{variant}-{kind}";binary.parent.mkdir(exist_ok=True)
            shutil.copy2(target/"debug/pipeline089",binary)
            assert run.sha(binary)==run.sha(target/"debug/pipeline089")
            run.write(out/f"logs/prune-{variant}-{kind}.json",dict(completed_reconstructible_target=str(target),retained_binary=str(binary),sha256=run.sha(binary)))
            shutil.rmtree(target)
        print(f"cold built {variant}: plain and counted",flush=True)
    inputs=out/"inputs";inputs.mkdir()
    run.extract(repo,run.VARIANTS["B"],["tests/perf/impl084","examples/mainline"],inputs)
    cases=json.loads((inputs/"tests/perf/impl084/cases.json").read_text())
    case_by_name={c["name"]:c for c in cases}
    for case in cases:
        folder=out/"edit-manifests"/case["name"];folder.mkdir(parents=True)
        for scenario,edits in case["edits"].items():
            (folder/(scenario+".tsv")).write_text("".join(f"{inputs/e['target']}\t{inputs/e['replacement']}\n" for e in edits))
    before=run.inventory(inputs)
    guards={}
    for variant in SELECTED:
        directory=out/f"guards-{variant}"
        run.invoke([out/"bin"/f"{variant}-plain","guards",directory,out/"guard-common"],out,out/f"logs/guards-{variant}")
        data=json.loads((directory/"report.json").read_text())
        assert data["status"]=="Pass" and data["schema"]==15 and data["production_commit"]==run.VARIANTS[variant]
        assert data["requests"]==142
        guards[variant]=dict(analysis_n=data["analysis_n"],ct_n=data["ct_n"],requests=data["requests"],records=[run.normalized(r) for r in data["records"]])
    assert guards["B"]==guards["KL"]
    run.write(out/"guard-controls.json",guards)
    checks.append(dict(name="142 complete source and filesystem guards per variant",status="Pass",requests=284))
    native_count=0;compiler_count=284
    for name in ["owned_component","helper_ui","byte_arithmetic"]:
        case=case_by_name[name];controls={}
        for variant in SELECTED:
            directory=out/f"native/{name}-{variant}"
            directory.parent.mkdir(parents=True,exist_ok=True)
            run.invoke([out/"bin"/f"{variant}-plain",directory,"build",inputs/case["root"],out/"edit-manifests"/name,case["capacity"]],out,out/f"logs/native-{name}-{variant}")
            data=json.loads((directory/"report.json").read_text())
            assert data["complete"] and data["schema"]==15 and not data["counting"]
            assert data["production_commit"]==run.VARIANTS[variant]
            controls[variant]=run.controls(directory,data,case,True)
            native_count+=4;compiler_count+=4
        assert controls["B"]==controls["KL"]
        checks.append(dict(name=name+" four complete builds and native behavior",status="Pass",requests=8,native_executions=8))
    counts=[]
    for name in ["modules64","modules512","modules64-default-capacity"]:
        case=case_by_name[name];controls={}
        for variant in SELECTED:
            directory=out/f"counts/{name}-{variant}"
            directory.parent.mkdir(parents=True,exist_ok=True)
            run.invoke([out/"bin"/f"{variant}-counted",directory,"verify",inputs/case["root"],out/"edit-manifests"/name,case["capacity"]],out,out/f"logs/counts-{name}-{variant}")
            data=json.loads((directory/"report.json").read_text())
            assert data["complete"] and data["schema"]==15 and data["counting"]
            assert data["production_commit"]==run.VARIANTS[variant]
            controls[variant]=run.controls(directory,data,case,False)
            expected=[len(r["sources"]) for r in data["records"]] if variant=="B" or name.endswith("default-capacity") else [len(data["records"][0]["sources"]),0,1,2]
            observed=[r["parser_calls"] for r in data["records"]]
            assert observed==expected and all(r["lexer_calls"]==r["parser_calls"] for r in data["records"])
            counts.append(dict(case=name,variant=variant,actual=observed,source_files=[len(r["sources"]) for r in data["records"]]))
            compiler_count+=4
        assert controls["B"]==controls["KL"]
        checks.append(dict(name=name+" direct lexer/parser counts and full Verify",status="Pass",requests=8))
    assert before==run.inventory(inputs)
    assert compiler_count==332 and native_count==24
    report=dict(status="Pass",scope="bounded cold functional replay; debug0/incremental0 compiler builds; historical performance is not resampled",schema=15,
        production_admission="NoGo",variants={v:run.VARIANTS[v] for v in SELECTED},harness_commit=HARNESS,
        replay_script_sha256=run.sha(__file__),binaries={f"{v}-{k}":run.sha(out/"bin"/f"{v}-{k}") for v in SELECTED for k in ["plain","counted"]},
        compiler_requests=compiler_count,native_executions=native_count,guard_requests=284,build_requests=24,counted_verify_requests=24,
        checks=checks,actual_parse_counts=counts,elapsed_seconds=time.time()-start,performance_resampled=False)
    run.write(out/"report.json",report)
    print(json.dumps({k:v for k,v in report.items() if k not in ["checks","binaries","actual_parse_counts"]},ensure_ascii=False))

if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--repo",type=Path,required=True);parser.add_argument("--out",type=Path,required=True);args=parser.parse_args()
    replay(args.repo,args.out)

#!/usr/bin/env python3
"""Freeze real compiler inputs, compare complete requests, then sample alone."""
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

BASE = "c4c307648087f2f9d23fb5d611638140751e61ed"
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1",
           CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
for key in ["CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
    ENV.pop(key, None)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n")

def invoke(command, cwd, prefix, env=None, timeout=300, required=True):
    prefix = Path(prefix)
    prefix.parent.mkdir(parents=True, exist_ok=True)
    start = time.perf_counter_ns()
    try:
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=ENV | (env or {}),
                                capture_output=True, timeout=timeout)
        record = dict(argv=list(map(str, command)), cwd=str(cwd), exit=result.returncode,
                      elapsed_ns=time.perf_counter_ns()-start)
        stdout, stderr = result.stdout, result.stderr
    except subprocess.TimeoutExpired as error:
        record = dict(argv=list(map(str, command)), cwd=str(cwd), exit=None, outcome="Timeout",
                      elapsed_ns=time.perf_counter_ns()-start)
        stdout, stderr = error.stdout or b"", error.stderr or b""
    prefix.with_suffix(".stdout").write_bytes(stdout)
    prefix.with_suffix(".stderr").write_bytes(stderr)
    write(prefix.with_suffix(".command.json"), record)
    if required and record["exit"] != 0:
        raise RuntimeError(f"command failed; retained {prefix}: {stderr[-1200:]!r}")
    return record

def extract(repo, commit, paths, destination):
    raw = subprocess.check_output(["git", "archive", "--format=tar", commit, *paths], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive.getmembers():
            p = Path(member.name)
            if p.is_absolute() or ".." in p.parts or not (member.isfile() or member.isdir()):
                raise RuntimeError(f"unsafe source archive {member.name}")
        archive.extractall(destination, filter="data")

def inventory(root):
    return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob("*")) if p.is_file()}

def controls(directory, data, case, run_native):
    rows = []
    for record in data["records"]:
        report = record["report"]
        if report["status"] != "Success":
            rows.append(dict(scenario=record["scenario"], status=report["status"],
                             phase=report["phase"], diagnostics=report["diagnostics"]))
            continue
        assert report["verification_runs"] == 1 and record["active_queries"] == 0
        artifacts = {key:sha(path) for key,path in report["artifacts"].items() if key != "executable"}
        native = None
        if run_native and "executable" in report["artifacts"]:
            p = directory / f"{record['scenario']}-native"
            result = subprocess.run([report["artifacts"]["executable"]], input=case["stdin"].encode(),
                                    capture_output=True, timeout=8, env=ENV)
            p.with_suffix(".stdout").write_bytes(result.stdout)
            p.with_suffix(".stderr").write_bytes(result.stderr)
            native = dict(exit=result.returncode, stdout=result.stdout.decode(), stderr=result.stderr.decode())
            write(p.with_suffix(".json"), dict(argv=[report["artifacts"]["executable"]], stdin=case["stdin"], **native))
            assert native == dict(exit=case["exit"], stdout=case["stdout"], stderr=""), (case["name"],native)
        rows.append(dict(scenario=record["scenario"],status=report["status"],
                         input=report["input_fingerprint"],
                         sources={row["path"]:sha(row["bytes_path"]) for row in record["sources"]},
                         artifacts=artifacts,interfaces=report["scalar_interfaces"],
                         query_events=report["query_events"],native=native))
    write(directory / "controls.json", rows)
    return rows

def request(out, variant, case, mode, tag, native=False):
    identity = json.loads((out / "identity.json").read_text())
    directory = out / tag / f"{case['name']}-{mode}-{variant}"
    binary = out / f"target-{variant}/release/pipeline080"
    root = out / "inputs" / case["root"]
    edit = out / "inputs" / case["edit"]
    before = sha(edit)
    command = [binary, directory, mode, root, edit]
    directory.parent.mkdir(parents=True, exist_ok=True)
    invocation = invoke(command, out, directory.parent / (directory.name + "-runner"), required=False, timeout=120)
    # Failure before report emission is infrastructure failure, never a limit.
    if not (directory / "report.json").exists():
        raise RuntimeError(f"runner emitted no actual report: {directory}")
    data = json.loads((directory / "report.json").read_text())
    assert data["production_commit"] == identity[variant + "_commit"] and data["schema"] == 13
    assert sha(edit) == before, "source edit was not restored"
    assert [row["scenario"] for row in data["records"]] == ["cold","unchanged","private_edit","public_edit"]
    assert bool(data["complete"]) == (invocation["exit"] == 0)
    control = controls(directory, data, case, native)
    return data, invocation, control

def prepare(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    for commit in [BASE,args.candidate]:
        actual = subprocess.check_output(["git","rev-parse","--verify",commit+"^{commit}"],cwd=repo,text=True).strip()
        assert actual == commit
    for variant,commit in [("baseline",BASE),("candidate",args.candidate)]:
        source = out / f"source-{variant}"
        source.mkdir()
        extract(repo,commit,["Cargo.toml","Cargo.lock","crates"],source)
        extract(repo,args.candidate,["tools/research/080"],source)
        write(out / f"source-{variant}-sha256.json",inventory(source))
        invoke(["cargo","build","--offline","--locked","--release","--manifest-path",source/"tools/research/080/Cargo.toml","--target-dir",out/f"target-{variant}"],
               source,out/f"logs/build-{variant}",env={"ZIL080_PRODUCTION_COMMIT":commit})
    inputs = out / "inputs"
    inputs.mkdir()
    extract(repo,args.candidate,["tests/perf/impl080","examples/mainline"],inputs)
    cases = json.loads((inputs/"tests/perf/impl080/cases.json").read_text())
    identity = dict(baseline_commit=BASE,candidate_commit=args.candidate,schema=13,
                    runner_sha256=sha(out/"source-candidate/tools/research/080/main.rs"),
                    controller_sha256=sha(__file__),case_manifest_sha256=sha(inputs/"tests/perf/impl080/cases.json"),
                    input_sha256=inventory(inputs),
                    binaries={v:sha(out/f"target-{v}/release/pipeline080") for v in ["baseline","candidate"]},
                    profile="release codegen-units1 debug0 incremental0 jobs1",
                    host=platform.platform(),warmup=1,rounds=7,cases=cases,
                    source_paths="same absolute frozen paths for both variants; every temporary edit restored",
                    cold="empty compiler session; OS file page cache not flushed",
                    formal_samples="NotRun")
    write(out/"identity.json",identity)
    results=[]
    for case in cases:
        for mode in case["modes"]:
            pair={}
            for variant in ["baseline","candidate"]:
                data,command,control=request(out,variant,case,mode+"-emit","preflight",native=True)
                pair[variant]=dict(complete=data["complete"],control=control,exit=command["exit"])
            if pair["baseline"]["complete"] and pair["candidate"]["complete"]:
                assert pair["baseline"]["control"]==pair["candidate"]["control"],(case["name"],mode,"full artifact mismatch")
                outcome="Complete"
            else:
                outcome="NonComplete"
            results.append(dict(case=case["name"],mode=mode,outcome=outcome,variants=pair))
            write(out/"preflight.json",results)
            print(f"preflight {case['name']} {mode}: {outcome}",flush=True)
    eligible=[dict(case=r["case"],mode=r["mode"]) for r in results if r["outcome"]=="Complete"]
    write(out/"eligibility.json",eligible)
    print(json.dumps(dict(preflight_cells=len(results),complete=len(eligible),not_complete=len(results)-len(eligible))))

def sample(args):
    out=args.out.resolve()
    identity=json.loads((out/"identity.json").read_text())
    assert not (out/"samples.json").exists()
    eligible=json.loads((out/"eligibility.json").read_text())
    for variant in ["baseline","candidate"]:
        assert sha(out/f"target-{variant}/release/pipeline080")==identity["binaries"][variant]
    assert inventory(out/"inputs")==identity["input_sha256"]
    cases={case["name"]:case for case in identity["cases"]}
    records=[]
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),eligible=eligible))
    for r in range(identity["warmup"]+identity["rounds"]):
        for cellno,cell in enumerate(eligible):
            case=cases[cell["case"]]
            variants=["baseline","candidate"] if (r+cellno)%2==0 else ["candidate","baseline"]
            pair={}
            for variant in variants:
                data,command,control=request(out,variant,case,cell["mode"],f"samples/r{r:02}",native=False)
                assert data["complete"]
                pair[variant]=control
                records.append(dict(round=r,warmup=r<identity["warmup"],case=case["name"],mode=cell["mode"],
                                    variant=variant,process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],records=data["records"]))
                write(out/"samples.json",records)
            assert pair["baseline"]==pair["candidate"],(cell,r,"same-input outputs differ")
        print(f"sampling round {r+1}/{identity['warmup']+identity['rounds']} complete",flush=True)
    write(out/"sample-finish.json",dict(status="Pass",window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),
                                      processes=len(records),requests=4*len(records),warmup_processes=sum(r["warmup"] for r in records),
                                      samples_sha256=sha(out/"samples.json")))
    analyze(out)

def analyze(out):
    rows=json.loads((out/"samples.json").read_text())
    cells=[]
    randomizer=random.Random(80)
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        pairs={v:sorted([r for r in rows if not r["warmup"] and r["case"]==case and r["mode"]==mode and r["variant"]==v],key=lambda r:r["round"]) for v in ["baseline","candidate"]}
        for scenario in ["cold","unchanged","private_edit","public_edit"]:
            values={v:[next(x["api_wall_ns"] for x in row["records"] if x["scenario"]==scenario) for row in rr] for v,rr in pairs.items()}
            ratios=[c/b for b,c in zip(values["baseline"],values["candidate"])]
            samples=sorted(statistics.median(randomizer.choices(ratios,k=len(ratios))) for _ in range(2000))
            cells.append(dict(case=case,mode=mode,scenario=scenario,api_ns=values,
                              medians_ns={v:statistics.median(a) for v,a in values.items()},
                              paired_ratio=statistics.median(ratios),exploratory_95=[samples[49],samples[1949]],
                              vmhwm_kib={v:[r["vmhwm_kib"] for r in rr] for v,rr in pairs.items()},
                              process_wall_ns={v:[r["process_wall_ns"] for r in rr] for v,rr in pairs.items()}))
    write(out/"summary.json",dict(status="Pass",sample_processes=len(rows),compiler_requests=4*len(rows),
                                 cells=cells,limitations=["single-host bounded result, no Rust-equivalent performance claim",
                                 "private/public edits add unused functions; not all editing workloads",
                                 "VmHWM includes four scenarios, outputs and whole session lifetime",
                                 "API intervals exclude edit writes and controller output; process wall includes runner recording and destruction",
                                 "bootstrap describes seven paired samples; no universal statistical guarantee"]))

def main():
    parser=argparse.ArgumentParser()
    sub=parser.add_subparsers(dest="operation",required=True)
    p=sub.add_parser("prepare");p.add_argument("--repo",type=Path,required=True);p.add_argument("--candidate",required=True);p.add_argument("--out",type=Path,required=True)
    p=sub.add_parser("sample");p.add_argument("--out",type=Path,required=True);p.add_argument("--window",required=True)
    p=sub.add_parser("analyze");p.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    if args.operation=="prepare":prepare(args)
    elif args.operation=="sample":sample(args)
    else:analyze(args.out.resolve())

if __name__=="__main__":main()

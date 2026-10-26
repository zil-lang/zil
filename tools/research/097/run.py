#!/usr/bin/env python3
"""Frozen B0/A/B/AB complete-request comparison, with separately built counters."""
from __future__ import annotations
import argparse, hashlib, io, json, os, platform, random, shutil, statistics, subprocess, tarfile, time
from pathlib import Path
from instrument import NAMES, instrument

BASE = "28dbeba242578bac0178c1b2a97ce82c01f78018"
A = "0b465124391cb5b286abd195a16a477b2a15b3d9"
AB = "82a81c75c5d7b6de6cbf6ff72182ac4947af29c1"
VARIANTS = ["B0", "A", "B", "AB"]
SCENARIOS = ["cold", "unchanged", "private_edit", "public_edit"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1",
           CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_DEV_DEBUG="0")
for key in ["CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
    ENV.pop(key, None)

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path, value):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
def inventory(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob("*")) if p.is_file()}
def invoke(argv, cwd, prefix, env=None, timeout=600, required=True, stdin=None):
    argv = list(map(str, argv)); prefix = Path(prefix); prefix.parent.mkdir(parents=True, exist_ok=True)
    began = time.perf_counter_ns()
    try:
        result = subprocess.run(argv, cwd=cwd, env=ENV | (env or {}), input=stdin,
                                capture_output=True, timeout=timeout)
        record = dict(argv=argv, cwd=str(cwd), exit=result.returncode, elapsed_ns=time.perf_counter_ns()-began)
        stdout, stderr = result.stdout, result.stderr
    except subprocess.TimeoutExpired as e:
        record = dict(argv=argv, cwd=str(cwd), exit=None, outcome="Timeout", elapsed_ns=time.perf_counter_ns()-began)
        stdout, stderr = e.stdout or b"", e.stderr or b""
    prefix.with_suffix(".stdout").write_bytes(stdout); prefix.with_suffix(".stderr").write_bytes(stderr)
    write(prefix.with_suffix(".command.json"), record)
    if required and record["exit"] != 0: raise RuntimeError(f"failed; preserved {prefix}: {stderr[-1600:]!r}")
    return record
def extract(repo, commit, paths, destination):
    raw = subprocess.check_output(["git", "archive", "--format=tar", commit, *paths], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        assert all(not Path(m.name).is_absolute() and ".." not in Path(m.name).parts
                   and (m.isfile() or m.isdir()) for m in tar.getmembers())
        tar.extractall(destination, filter="data")

def inputs(repo, out):
    root = out / "inputs"; root.mkdir()
    extract(repo, BASE, ["examples/mainline/owned_component", "examples/mainline/byte_buffer"], root)
    cases=[]
    for name, n in [("tiny",0), ("modules512",512)]:
        folder=root/name;folder.mkdir()
        text="".join(f"import m{i}\n" for i in range(n))
        text += "fn main()->i32=>"+("m0.value()" if n else "0")+"\n"
        (folder/"main.zil").write_text(text)
        for i in range(n): (folder/f"m{i}.zil").write_text("pub fn value()->i32=>0\n")
        cases.append(dict(name=name,root=f"{name}/main.zil",edit=f"{name}/m0.zil" if n else f"{name}/main.zil",
                          modes=["verify"],capacity=32 if n==0 else 1024,stdin="",stdout=""))
    for reverse in [False,True]:
        name="reverse128" if reverse else "forward128"
        folder=root/name;folder.mkdir();text="resource Token{n:i32}\n"
        for i in (range(128) if reverse else reversed(range(128))):
            body=f"f{i+1}(r)" if i+1<128 else "r.n+=1"
            text+=f"fn f{i}(r:Token){{{body}}}\n"
        text+="fn main()->i32{let r=Token{n:0};let c=()=>{f0(r);r.n};return c()+c()-3;}\n"
        (folder/"main.zil").write_text(text)
        cases.append(dict(name=name,root=f"{name}/main.zil",edit=f"{name}/main.zil",modes=["verify"],capacity=1024,stdin="",stdout=""))
    for name,directory,edit,stdin,stdout in [
        ("owned-main","owned_component","window.zil","+r-q","0\n1\nS1\n0\n#4\nMSU"),
        ("bytes-main","byte_buffer","buffer.zil","ab\x00z\n","AB\x00Z\n"),
    ]:
        cases.append(dict(name=name,root=f"examples/mainline/{directory}/main.zil",edit=f"examples/mainline/{directory}/{edit}",
                          modes=["verify","build"],capacity=1024,stdin=stdin,stdout=stdout))
    for case in cases:
        target=root/case["edit"];original=target.read_text()
        folder=out/"edit-manifests"/case["name"];folder.mkdir(parents=True)
        private=original+"\nfn research097_private()->i32=>17\n"
        for scenario,text in [("private_edit",private),("public_edit",private+"\npub fn research097_public()->i32=>29\n")]:
            replacement=folder/f"{scenario}.zil";replacement.write_text(text)
            (folder/f"{scenario}.tsv").write_text(f"{target}\t{replacement}\n")
    assert sum(len(c["modes"]) for c in cases)==8
    return cases

def normalized(row):
    report=row["report"]
    value={k:v for k,v in report.items() if k not in ["seconds","commands","artifacts"]}
    value["artifacts"]={k:sha(p) for k,p in report["artifacts"].items() if k!="executable"}
    value.update({k:v for k,v in row.items() if k not in ["report","api_wall_ns","counts"]})
    return value
def request(out,variant,case,mode,tag,counted=False,native=False):
    identity=json.loads((out/"identity.json").read_text())
    folder=out/tag/f"{case['name']}-{mode}-{variant}"
    binary=out/"bin"/f"{variant}-{'counted' if counted else 'timed'}"
    source=out/"inputs"/case["edit"];before=sha(source)
    argv=[binary,folder,mode,out/"inputs"/case["root"],out/"edit-manifests"/case["name"],case["capacity"]]
    command=invoke(argv,out,folder.parent/(folder.name+"-runner"),timeout=180)
    data=json.loads((folder/"report.json").read_text())
    assert data["production_identity"]==identity["variants"][variant]["identity"] and data["schema"]==17
    assert data["counting"]==counted and data["complete"] and data["capacity"]==case["capacity"]
    assert sha(source)==before and [r["scenario"] for r in data["records"]]==SCENARIOS
    controls=[]
    for r in data["records"]:
        assert r["report"]["verification_runs"]==1 and r["active_queries"]==0
        row=normalized(r)
        if native and "executable" in r["report"]["artifacts"]:
            prefix=folder/(r["scenario"]+"-native")
            observed=invoke([r["report"]["artifacts"]["executable"]],out,prefix,stdin=case["stdin"].encode(),timeout=20)
            assert observed["exit"]==0 and prefix.with_suffix(".stdout").read_bytes()==case["stdout"].encode()
            assert prefix.with_suffix(".stderr").read_bytes()==b""
            row["native"]=dict(exit=observed["exit"],stdout_sha256=sha(prefix.with_suffix(".stdout")))
        controls.append(row)
    fingerprints=[r["report"]["input_fingerprint"] for r in data["records"]]
    assert fingerprints[0]==fingerprints[1] and len(set(fingerprints))==3
    signatures=[[i["signature_fingerprint"] for i in r["report"]["scalar_interfaces"]] for r in data["records"]]
    assert signatures[0]==signatures[2] and signatures[2]!=signatures[3]
    assert any(e["disposition"]=="reused" for e in data["records"][1]["report"]["query_events"])
    write(folder/"control-hash.json",dict(sha256=hashlib.sha256(json.dumps(controls,sort_keys=True,separators=(",",":")).encode()).hexdigest(), normalization="normalized() in frozen controller; original full report retained"))
    return data,command,controls

def prepare(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False);repo=args.repo.resolve()
    variants={"B0":dict(commit=BASE,identity=BASE),"A":dict(commit=A,identity=A),
              "B":dict(commit=BASE,overlay=dict(commit=AB,paths=["crates/zil-db/src/lib.rs"]),identity=BASE+"+db@"+AB),
              "AB":dict(commit=AB,identity=AB)}
    cases=inputs(repo,out)
    write(out/"preregistered-inputs.json",dict(cases=cases,warmup=1,rounds=7,variants=variants,
        schema=17,input_sha256=inventory(out/"inputs"),controller_sha256=sha(__file__),
        formal_samples="NotRun",stop="Any semantic mismatch; retain all cells; report each candidate separately"))
    for variant,record in variants.items():
        source=out/f"source-{variant}";source.mkdir()
        extract(repo,record["commit"],["Cargo.toml","Cargo.lock","crates"],source)
        if "overlay" in record:
            extract(repo,record["overlay"]["commit"],record["overlay"]["paths"],source)
        extract(repo,args.harness,["tools/research/097","tools/research/089"],source)
        write(out/f"source-{variant}-sha256.json",inventory(source))
        counted=out/f"source-{variant}-counted";shutil.copytree(source,counted);instrument(counted)
        write(out/f"source-{variant}-counted-sha256.json",inventory(counted))
        for kind,tree in [("timed",source),("counted",counted)]:
            target=out/f"target-{variant}-{kind}"
            env={"ZIL097_PRODUCTION_IDENTITY":record["identity"],"ZIL089_PRODUCTION_COMMIT":record["identity"],
                 "RUSTFLAGS":"--check-cfg=cfg(zil097_count)"+(" --cfg zil097_count" if kind=="counted" else "")}
            invoke(["cargo","build","--offline","--locked","--release","--manifest-path",tree/"tools/research/097/Cargo.toml","--target-dir",target],tree,out/f"logs/build-{variant}-{kind}",env)
            dest=out/"bin"/f"{variant}-{kind}";dest.parent.mkdir(exist_ok=True)
            shutil.copy2(target/"release/pipeline097",dest)
            assert sha(dest)==sha(target/"release/pipeline097")
            write(out/f"logs/prune-{variant}-{kind}.json",dict(path=str(target),retained_binary=str(dest),sha256=sha(dest),purpose="rebuildable scoped Cargo target only"))
            shutil.rmtree(target)
            print(f"built {variant} {kind}",flush=True)
    identity=dict(variants=variants,harness_commit=args.harness,schema=17,controller_sha256=sha(__file__),instrument_sha256=sha(Path(__file__).with_name("instrument.py")),
        binaries={f"{v}-{k}":sha(out/"bin"/f"{v}-{k}") for v in VARIANTS for k in ["timed","counted"]},
        input_sha256=inventory(out/"inputs"),edit_sha256=inventory(out/"edit-manifests"),cases=cases,counter_names=NAMES,warmup=1,rounds=7,formal_samples="NotRun",
        rustc=subprocess.check_output(["rustc","-Vv"],text=True),cargo=subprocess.check_output(["cargo","-V"],text=True),
        clang=subprocess.check_output([ENV.get("ZIL_CLANG","clang"),"--version"],text=True),host=platform.platform(),
        profile="release codegen-units1 debug0 incremental0 jobs1",environment={k:ENV[k] for k in ENV if k.startswith("CARGO_")})
    write(out/"identity.json",identity)
    functional(out)

def functional(out):
    identity=json.loads((out/"identity.json").read_text());all_guards={};cells=[];count_rows=[]
    for variant in VARIANTS:
        folder=out/f"guards-{variant}"
        invoke([out/"bin"/f"{variant}-timed","guards",folder,out/"guard-common"],out,out/f"logs/guards-{variant}")
        data=json.loads((folder/"report.json").read_text())
        assert data["production_commit"]==identity["variants"][variant]["identity"] and data["schema"]==17 and data["status"]=="Pass"
        all_guards[variant]=dict(analysis_n=data["analysis_n"],ct_n=data["ct_n"],requests=data["requests"],records=[normalized(r) for r in data["records"]])
        write(out/"guard-controls.json",all_guards)
    # B preserves all existing budget observations. A changes measured frontier
    # work, so its exact N/N-1 and budget-search reports are compared against AB.
    assert all_guards["B0"]==all_guards["B"]
    assert all_guards["A"]==all_guards["AB"]
    ordinary=lambda g:[r for r in g["records"] if not r["scenario"].startswith(("search-analysis-","budget-analysis-"))]
    assert ordinary(all_guards["B0"])==ordinary(all_guards["A"])
    for case in identity["cases"]:
        for mode in case["modes"]:
            controls={};counters={}
            for variant in VARIANTS:
                _,_,controls[variant]=request(out,variant,case,mode+"-emit","preflight",native=True)
                counted,_,count_control=request(out,variant,case,mode,"counts",counted=True)
                trim=lambda rows:[{k:v for k,v in r.items() if k not in ["artifacts","native"]} for r in rows]
                assert trim(controls[variant])==trim(count_control),(case["name"],mode,variant,"instrumentation changed semantics")
                counters[variant]=[r["counts"] for r in counted["records"]]
            assert all(controls["B0"]==controls[v] for v in VARIANTS),(case["name"],mode,"artifact/semantic mismatch")
            # Scheduling may change Types visits, Database may change read work;
            # Analyze and independent Verify retain their exact production work.
            assert all([c[6:] for c in counters["B0"]]==[c[6:] for c in counters[v]] for v in VARIANTS)
            count_rows.append(dict(case=case["name"],mode=mode,counts=counters))
            cells.append(dict(case=case["name"],mode=mode,status="Pass"))
            write(out/"counts.json",dict(names=NAMES,cells=count_rows))
            write(out/"functional.json",dict(status="Pass" if len(cells)==8 else "Running",cells=cells,
                guard_requests={v:g["requests"] for v,g in all_guards.items()},requests=len(cells)*32,native_executions=32 if len(cells)==8 else None,
                budget_boundaries={v:dict(analysis_n=g["analysis_n"],ct_n=g["ct_n"]) for v,g in all_guards.items()}))
            print(f"functional {case['name']} {mode} Pass",flush=True)

def verify(out):
    identity=json.loads((out/"identity.json").read_text())
    assert sha(__file__)==identity["controller_sha256"] and sha(Path(__file__).with_name("instrument.py"))==identity["instrument_sha256"]
    for variant in VARIANTS:
        for kind in ["timed","counted"]:
            assert sha(out/"bin"/f"{variant}-{kind}")==identity["binaries"][f"{variant}-{kind}"]
            suffix="" if kind=="timed" else "-counted"
            assert inventory(out/f"source-{variant}{suffix}")==json.loads((out/f"source-{variant}{suffix}-sha256.json").read_text())
    assert inventory(out/"inputs")==identity["input_sha256"] and inventory(out/"edit-manifests")==identity["edit_sha256"]
    assert json.loads((out/"functional.json").read_text())["status"]=="Pass"
    return identity
def sample(args):
    out=args.out.resolve();identity=verify(out)
    assert not (out/"samples.json").exists()
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),cpu_affinity=sorted(os.sched_getaffinity(0))))
    records=[]
    for r in range(8):
        for cellno,(case,mode) in enumerate((c,m) for c in identity["cases"] for m in c["modes"]):
            # Four-way balanced rotation, reversed every second block.
            offset=(r+cellno)%4;order=VARIANTS[offset:]+VARIANTS[:offset]
            if ((r+cellno)//4)%2:order=list(reversed(order))
            controls={}
            for variant in order:
                data,command,controls[variant]=request(out,variant,case,mode,f"samples/r{r:02}")
                records.append(dict(round=r,warmup=r==0,case=case["name"],mode=mode,variant=variant,
                    process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],
                    report_path=str(out/f"samples/r{r:02}"/f"{case['name']}-{mode}-{variant}"/"report.json"),
                    records=[{k:x[k] for k in ["scenario","api_wall_ns","counts","active_queries","cached_queries"]} for x in data["records"]]))
                write(out/"samples.json",records)
            assert all(controls["B0"]==controls[v] for v in VARIANTS)
        print(f"sample round {r} complete",flush=True)
    assert len(records)==256
    write(out/"sample-finish.json",dict(status="Pass",window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),
        processes=256,requests=1024,warmup_processes=32,retained_processes=224,samples_sha256=sha(out/"samples.json")))
    analyze(out)
def analyze(out):
    rows=json.loads((out/"samples.json").read_text());rng=random.Random(97);cells=[];processes=[]
    assert len(rows)==256 and sum(not r["warmup"] for r in rows)==224
    def stats(values):
        comparisons={}
        for left,right in [("B0","A"),("B0","B"),("B0","AB"),("A","AB"),("B","AB")]:
            ratios=[r/l for l,r in zip(values[left],values[right])]
            draws=sorted(statistics.median(rng.choices(ratios,k=7)) for _ in range(2000))
            comparisons[f"{right}/{left}"]=dict(paired_ratio=statistics.median(ratios),exploratory_95=[draws[49],draws[1949]])
        return dict(values=values,medians={v:statistics.median(x) for v,x in values.items()},comparisons=comparisons)
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        data={v:sorted([r for r in rows if not r["warmup"] and (r["case"],r["mode"],r["variant"])==(case,mode,v)],key=lambda r:r["round"]) for v in VARIANTS}
        assert all(len(x)==7 for x in data.values())
        for scenario in SCENARIOS:
            values={v:[next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario) for r in rr] for v,rr in data.items()}
            cells.append(dict(case=case,mode=mode,scenario=scenario,**stats(values)))
        processes.append(dict(case=case,mode=mode,**stats({v:[r["process_wall_ns"] for r in rr] for v,rr in data.items()}),vmhwm_kib={v:[r["vmhwm_kib"] for r in rr] for v,rr in data.items()}))
    regressions={v:[dict(case=r["case"],mode=r["mode"],scenario=r.get("scenario","process"),**r["comparisons"][f"{v}/B0"])
                    for r in cells+processes if r["comparisons"][f"{v}/B0"]["exploratory_95"][0]>1] for v in ["A","B","AB"]}
    write(out/"summary.json",dict(status="Pass",sample_processes=256,compiler_requests=1024,cells=cells,processes=processes,regressions=regressions,
        production_admission={v:"NoGo" if regressions[v] else "IndependentReviewRequired" for v in ["A","B","AB"]},
        limitations=["7 paired samples; exploratory intervals, no multiplicity correction","bounded same-host result; no Rust-equivalent speed claim",
                     "cold means empty compiler session, not flushed OS cache","parent VmHWM excludes clang child peak","private/public edits append unused functions",
                     "B-only source is exact baseline plus the independently frozen Database production-file overlay; all source hashes retained"]))

if __name__=="__main__":
    p=argparse.ArgumentParser();sub=p.add_subparsers(dest="operation",required=True)
    q=sub.add_parser("prepare");q.add_argument("--repo",type=Path,required=True);q.add_argument("--harness",required=True);q.add_argument("--out",type=Path,required=True)
    q=sub.add_parser("sample");q.add_argument("--out",type=Path,required=True);q.add_argument("--window",required=True)
    for name in ["functional","analyze","verify"]:
        q=sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args=p.parse_args()
    if args.operation=="prepare":prepare(args)
    elif args.operation=="sample":sample(args)
    elif args.operation=="functional":functional(args.out.resolve())
    elif args.operation=="analyze":analyze(args.out.resolve())
    elif args.operation=="verify":verify(args.out.resolve())

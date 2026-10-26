#!/usr/bin/env python3
"""Registered 101 BASE/R full-cost confirmation; no inference from counters."""
from __future__ import annotations
import argparse, hashlib, json, math, os, platform, random, shutil, statistics, subprocess, time
from pathlib import Path
from probe import NAMES, instrument
from support import BASE, ENV, legacy, sha, write, inventory, invoke, extract, normalize, compact_report, archive_directory

VARIANTS = ["BASE", "R"]
WARMUP, ROUNDS, FAMILY = 2, 64, 95
ALPHA, MARGIN = .05, 1.01
ORDINARY = ["cold", "unchanged", "private_edit", "public_edit"]
USED_BODY = ["cold", "unchanged", "body_edit", "body_restore"]
SCRIPT_NAMES = ["run.py", "probe.py", "support.py", "main.rs", "Cargo.toml", "Cargo.lock"]


def manifest(out, case, scenario, replacement):
    folder = out / "edit-manifests" / case["name"]
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{scenario}.zil"
    path.write_text(replacement)
    (folder / f"{scenario}.tsv").write_text(f"{out / 'inputs' / case['edit']}\t{path}\n")


def inputs(repo, out):
    cases = legacy.inputs(repo, out)
    for case in cases:
        case["scenarios"] = ORDINARY
    for name in ["swap16", "alias-cycle16", "fanjoin16"]:
        text = "resource Token{n:i32}\n"
        for i in range(16):
            if name == "swap16":
                body = f"f{i+1}(b,a)" if i < 15 else "a.n+=1"
                text += f"fn f{i}(a:Token,b:Token){{{body}}}\n"
            elif name == "alias-cycle16":
                body = "let alias=a;for step in 0..1 {if n>0 {" + f"f{(i+1)%16}(b,alias,n-1)" + "}}"
                if i == 15:
                    body += ";a.n+=1"
                text += f"fn f{i}(a:Token,b:Token,n:i32){{{body}}}\n"
            else:
                body = f"f{i+1}(a,b);f{i+2}(b,a)" if i < 14 else ("f15(a,b)" if i == 14 else "a.n+=1")
                text += f"fn f{i}(a:Token,b:Token){{{body}}}\n"
        args = "a,b,0" if name == "alias-cycle16" else "a,b"
        text += "fn main()->i32{let a=Token{n:0};let b=Token{n:0};let cb=()=>{" + f"f0({args});a.n+b.n" + "};return cb()+cb()-3;}\n"
        folder = out / "inputs" / name
        folder.mkdir()
        (folder / "main.zil").write_text(text)
        case = dict(name=name, root=f"{name}/main.zil", edit=f"{name}/main.zil", modes=["verify"],
                    capacity=1024, stdin="", stdout="", scenarios=ORDINARY)
        private = text + "\nfn research101_private()->i32=>17\n"
        manifest(out, case, "private_edit", private)
        manifest(out, case, "public_edit", private + "\npub fn research101_public()->i32=>29\n")
        cases.append(case)
    changes = {
        "owned-main": ("state.count += 1", "state.count += 2", "0\n2\nS2\n1\n#4\nMSU"),
        "bytes-main": ("var next: i64 = 8", "var next: i64 = 16", "AB\x00Z\n"),
        "owned-bytes-main": ("state.sum += i64(byte)", "state.sum += i64(byte) * 2",
                             "".join(chr(3*i) for i in range(1,21)) + "\x00" + "".join(chr(5*i) for i in range(1,5))),
    }
    for parent in list(cases):
        if parent["name"] not in changes:
            continue
        case = dict(parent, name=parent["name"]+"-used", scenarios=USED_BODY)
        before, after, expected = changes[parent["name"]]
        original = (out / "inputs" / case["edit"]).read_text()
        assert original.count(before) == 1, (parent["name"], before)
        case["stdout_by_scenario"] = {s: expected if s == "body_edit" else parent["stdout"] for s in USED_BODY}
        case["actual_used_edit"] = dict(before=before, after=after)
        manifest(out, case, "body_edit", original.replace(before, after))
        manifest(out, case, "body_restore", original)
        cases.append(case)
    for case in cases:
        (out / "edit-manifests" / case["name"] / "scenarios.txt").write_text("\n".join(case["scenarios"]) + "\n")
    assert sum(len(c["modes"]) for c in cases) == 19
    return cases


def schedule(cases):
    rng = random.Random(101)
    cells = [(c["name"], mode) for c in cases for mode in c["modes"]]
    orders = {}
    for case, mode in cells:
        values = [False] * 32 + [True] * 32
        rng.shuffle(values)
        orders[(case, mode)] = values
    rows = []
    for r in range(WARMUP + ROUNDS):
        for index, (case, mode) in enumerate(cells):
            reverse = bool((r+index) % 2) if r < WARMUP else orders[(case, mode)][r-WARMUP]
            rows.append(dict(round=r, warmup=r<WARMUP, case=case, mode=mode,
                             order=list(reversed(VARIANTS)) if reverse else VARIANTS[:]))
    assert len(rows) == 1254
    for case, mode in cells:
        assert sum(row["order"][0] == "BASE" for row in rows if not row["warmup"] and (row["case"],row["mode"]) == (case,mode)) == 32
    return rows


def request(out, variant, case, mode, tag, counted=False, native=False):
    identity = json.loads((out / "identity.json").read_text())
    folder = out / tag / f"{case['name']}-{mode}-{variant}"
    key = f"{variant}-{'counted' if counted else 'timed'}"
    source = out / "inputs" / case["edit"]
    before = sha(source)
    command = invoke([out/"bin"/key, folder, mode, out/"inputs"/case["root"],
                      out/"edit-manifests"/case["name"], case["capacity"]],
                     out, folder.parent/(folder.name+"-runner"), timeout=180)
    data = json.loads((folder / "report.json").read_text())
    assert data["production_identity"] == identity["variants"][variant]["identity"] and data["schema"] == 17
    assert data["counting"] == counted and not data["diagnostic"] and data["complete"]
    assert data["capacity"] == case["capacity"] and sha(source) == before
    assert [r["scenario"] for r in data["records"]] == case["scenarios"]
    controls = []
    for row in data["records"]:
        assert row["report"]["verification_runs"] == 1 and row["active_queries"] == 0
        control = normalize(row)
        if native and "executable" in row["report"]["artifacts"]:
            prefix = folder / (row["scenario"] + "-native")
            execution = invoke([row["report"]["artifacts"]["executable"]], out, prefix,
                               stdin=case["stdin"].encode(), timeout=20)
            expected = case.get("stdout_by_scenario", {}).get(row["scenario"], case["stdout"])
            assert execution["exit"] == 0 and prefix.with_suffix(".stdout").read_bytes() == expected.encode(), (case["name"],row["scenario"],"native output")
            assert prefix.with_suffix(".stderr").read_bytes() == b""
            control["native"] = dict(exit=0, stdout_sha256=sha(prefix.with_suffix(".stdout")))
        controls.append(control)
    fingerprints = [r["report"]["input_fingerprint"] for r in data["records"]]
    signatures = [[i["signature_fingerprint"] for i in r["report"]["scalar_interfaces"]] for r in data["records"]]
    if case["scenarios"] == USED_BODY:
        assert fingerprints[0] == fingerprints[1] == fingerprints[3] and fingerprints[2] != fingerprints[0]
        assert all(sig == signatures[0] for sig in signatures)
    else:
        assert fingerprints[0] == fingerprints[1] and len(set(fingerprints)) == 3
        assert signatures[0] == signatures[2] and signatures[2] != signatures[3]
    assert any(e["disposition"] == "reused" for e in data["records"][1]["report"]["query_events"])
    write(folder/"control-hash.json", dict(sha256=hashlib.sha256(json.dumps(controls,sort_keys=True,separators=(",",":")).encode()).hexdigest(),
                                         normalization="normalize() in frozen support.py; full report bytes preserved"))
    compact_report(folder/"report.json")
    return data, command, controls


def prepare(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    variants = {"BASE": dict(commit=BASE, identity=BASE), "R": dict(commit=args.candidate, identity=args.candidate)}
    cases = inputs(repo, out)
    write(out/"schedule.json", schedule(cases))
    write(out/"preregistered-inputs.json", dict(cases=cases, variants=variants, warmup=WARMUP, rounds=ROUNDS,
        family=FAMILY, margin=MARGIN, alpha=ALPHA, input_sha256=inventory(out/"inputs"),
        edits_sha256=inventory(out/"edit-manifests"), schedule_sha256=sha(out/"schedule.json"),
        harness=args.harness, formal_admission="NotRun", stop="Retain all cells and failures; never extend or replace samples"))
    for variant, spec in variants.items():
        for kind in ["timed", "counted"]:
            key = f"{variant}-{kind}"
            tree = out / ("source-" + key)
            tree.mkdir()
            extract(repo, spec["commit"], ["Cargo.toml", "Cargo.lock", "crates"], tree)
            extract(repo, args.harness, ["tools/research/101", "tools/research/100", "tools/research/089"], tree)
            if kind == "counted":
                instrument(tree)
            write(out/("source-"+key+"-sha256.json"), inventory(tree))
            target = out / ("target-"+key)
            flags = "--check-cfg=cfg(zil101_count) --check-cfg=cfg(zil101_diag)" + (" --cfg zil101_count" if kind == "counted" else "")
            env = {"ZIL101_PRODUCTION_IDENTITY": spec["identity"], "ZIL089_PRODUCTION_COMMIT": spec["identity"], "RUSTFLAGS": flags}
            invoke(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", tree/"tools/research/101/Cargo.toml", "--target-dir", target],
                   tree, out/f"logs/build-{key}", env)
            dest = out/"bin"/key
            dest.parent.mkdir(exist_ok=True)
            shutil.copy2(target/"release/pipeline101", dest)
            assert sha(dest) == sha(target/"release/pipeline101")
            write(out/f"logs/prune-{key}.json", dict(path=str(target), retained_binary=str(dest), sha256=sha(dest), scope="reproducible Cargo target only"))
            shutil.rmtree(target)
            print("built", key, flush=True)
    identity = dict(variants=variants, harness=args.harness, schema=17, cases=cases,
        scripts={name:sha(Path(__file__).with_name(name)) for name in SCRIPT_NAMES},
        binaries={f"{v}-{k}":sha(out/"bin"/f"{v}-{k}") for v in VARIANTS for k in ["timed", "counted"]},
        input_sha256=inventory(out/"inputs"), edits_sha256=inventory(out/"edit-manifests"), schedule_sha256=sha(out/"schedule.json"),
        counters=NAMES, warmup=WARMUP, rounds=ROUNDS, family=FAMILY, alpha=ALPHA, margin=MARGIN,
        formal_admission="NotRun", rustc=subprocess.check_output(["rustc","-Vv"],text=True),
        cargo=subprocess.check_output(["cargo","-V"],text=True), clang=subprocess.check_output([ENV.get("ZIL_CLANG","clang"),"--version"],text=True),
        host=platform.platform(), profile="release codegen-units1 debug0 incremental0 jobs1",
        environment={k:ENV[k] for k in ENV if k.startswith("CARGO_")})
    write(out/"identity.json", identity)
    functional(out)


def functional(out):
    identity = json.loads((out/"identity.json").read_text())
    guards, cells, counts = {}, [], []
    for variant in VARIANTS:
        folder = out / f"guards-{variant}"
        invoke([out/"bin"/f"{variant}-timed", "guards", folder, out/"guard-common"], out, out/f"logs/guards-{variant}")
        data = json.loads((folder/"report.json").read_text())
        assert data["production_commit"] == identity["variants"][variant]["identity"] and data["schema"] == 17 and data["status"] == "Pass"
        guards[variant] = dict(analysis_n=data["analysis_n"], ct_n=data["ct_n"], requests=data["requests"], records=[normalize(r) for r in data["records"]])
        write(out/"guard-controls.json", guards)
    ordinary = lambda g: [r for r in g["records"] if not r["scenario"].startswith(("search-analysis-", "budget-analysis-"))]
    assert ordinary(guards["BASE"]) == ordinary(guards["R"])
    for case in identity["cases"]:
        for mode in case["modes"]:
            controls, counters = {}, {}
            for variant in VARIANTS:
                _, _, controls[variant] = request(out,variant,case,mode+"-emit","preflight",native=True)
                data, _, counted = request(out,variant,case,mode,"counts",counted=True)
                trim = lambda rows: [{k:v for k,v in r.items() if k not in ["artifacts", "native"]} for r in rows]
                assert trim(controls[variant]) == trim(counted), (case["name"],mode,variant,"counting changed observations")
                counters[variant] = [r["counts"] for r in data["records"]]
            assert controls["BASE"] == controls["R"], (case["name"],mode,"semantic/artifact/native mismatch")
            assert [c[1:4] for c in counters["BASE"]] == [c[1:4] for c in counters["R"]], "DB work changed"
            assert [c[6:8] for c in counters["BASE"]] == [c[6:8] for c in counters["R"]], "Analyze/Verify work changed"
            for variant in VARIANTS:
                archive_directory(out/"preflight"/f"{case['name']}-{mode}-emit-{variant}")
            counts.append(dict(case=case["name"],mode=mode,counts=counters))
            cells.append(dict(case=case["name"],mode=mode,status="Pass"))
            write(out/"counts.json",dict(names=NAMES,cells=counts))
            write(out/"functional.json",dict(status="Pass" if len(cells)==19 else "Running",scope="bounded full-request differential controls",cells=cells,
                requests=len(cells)*16, native_executions=48 if len(cells)==19 else None,
                guard_requests={v:g["requests"] for v,g in guards.items()},
                budget_boundaries={v:dict(analysis_n=g["analysis_n"],ct_n=g["ct_n"]) for v,g in guards.items()}))
            print("functional",case["name"],mode,"Pass",flush=True)


def verify(out):
    identity = json.loads((out/"identity.json").read_text())
    for name, digest in identity["scripts"].items():
        assert sha(Path(__file__).with_name(name)) == digest, name
    for key, digest in identity["binaries"].items():
        assert sha(out/"bin"/key) == digest
        assert inventory(out/("source-"+key)) == json.loads((out/("source-"+key+"-sha256.json")).read_text())
    assert inventory(out/"inputs") == identity["input_sha256"]
    assert inventory(out/"edit-manifests") == identity["edits_sha256"]
    assert sha(out/"schedule.json") == identity["schedule_sha256"]
    assert json.loads((out/"schedule.json").read_text()) == schedule(identity["cases"])
    assert json.loads((out/"functional.json").read_text())["status"] == "Pass"
    return identity


def sample(args):
    out = args.out.resolve()
    identity = verify(out)
    assert not (out/"samples.json").exists() and not (out/"sample-start.json").exists()
    write(out/"sample-start.json",dict(window=args.window,utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),cpu_affinity=sorted(os.sched_getaffinity(0))))
    records = []
    cases = {c["name"]:c for c in identity["cases"]}
    cells = json.loads((out/"schedule.json").read_text())
    for index, cell in enumerate(cells):
        controls = {}
        for position,variant in enumerate(cell["order"]):
            data,command,controls[variant] = request(out,variant,cases[cell["case"]],cell["mode"],f"samples/r{cell['round']:02}")
            records.append(dict(round=cell["round"],warmup=cell["warmup"],case=cell["case"],mode=cell["mode"],variant=variant,position=position,
                process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],session_drop_ns=data["session_drop_ns"],
                report_path=str(out/f"samples/r{cell['round']:02}"/f"{cell['case']}-{cell['mode']}-{variant}"/"report.json.gz"),
                records=[{k:x[k] for k in ["scenario","api_wall_ns","counts","active_queries","cached_queries","observation_ns"]} for x in data["records"]]))
            write(out/"samples.json",records)
        assert controls["BASE"] == controls["R"], (cell,"semantic mismatch during measurement")
        if (index+1)%19 == 0:
            print("confirmation round",cell["round"],"complete",flush=True)
    assert len(records) == 2508 and sum(not r["warmup"] for r in records) == 2432
    write(out/"sample-finish.json",dict(status="Pass",scope="acquisition completed; admission separate",window=args.window,
        utc=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime()),processes=2508,requests=10032,
        warmup_processes=76,retained_processes=2432,samples_sha256=sha(out/"samples.json")))
    print("confirmation acquisition complete; analyze only after releasing exclusive window",flush=True)


def analyze(out):
    rows = json.loads((out/"samples.json").read_text())
    assert len(rows) == 2508 and sum(not r["warmup"] for r in rows) == 2432
    k = max(k for k in range(ROUNDS//2) if 2*sum(math.comb(ROUNDS,j) for j in range(k+1))/2**ROUNDS <= ALPHA/FAMILY)
    assert k == 17
    cells, processes = [], []
    def stats(values, arms):
        ratios = [b/a for a,b in zip(values["BASE"],values["R"])]
        ordered = sorted(ratios)
        return dict(values=values,medians={v:statistics.median(x) for v,x in values.items()},paired_ratios=ratios,
                    paired_median=statistics.median(ratios),simultaneous_95=[ordered[k],ordered[ROUNDS-k-1]],
                    ratio_by_order={v:[ratio for ratio,row in zip(ratios,arms[v]) if row["position"]==0] for v in VARIANTS},
                    ratio_by_time_half=[ratios[:32],ratios[32:]])
    for case,mode in sorted({(r["case"],r["mode"]) for r in rows}):
        arms = {v:sorted([r for r in rows if not r["warmup"] and (r["case"],r["mode"],r["variant"])==(case,mode,v)],key=lambda r:r["round"]) for v in VARIANTS}
        assert all(len(x)==ROUNDS for x in arms.values())
        for scenario in [x["scenario"] for x in arms["BASE"][0]["records"]]:
            values = {v:[next(x["api_wall_ns"] for x in r["records"] if x["scenario"]==scenario) for r in arm] for v,arm in arms.items()}
            cells.append(dict(case=case,mode=mode,scenario=scenario,**stats(values,arms)))
        processes.append(dict(case=case,mode=mode,scenario="process",**stats({v:[r["process_wall_ns"] for r in arm] for v,arm in arms.items()},arms),
                              vmhwm_kib={v:[r["vmhwm_kib"] for r in arm] for v,arm in arms.items()}))
    all_cells = cells + processes
    assert len(cells)==76 and len(processes)==19 and len(all_cells)==FAMILY
    primary = next(r for r in processes if r["case"]=="reverse128" and r["mode"]=="verify")
    short = lambda r: {k:r[k] for k in ["case","mode","scenario","paired_median","simultaneous_95"]}
    regressions = [short(r) for r in all_cells if r["simultaneous_95"][0] > MARGIN]
    uncertain = [short(r) for r in all_cells if r["simultaneous_95"][1] > MARGIN]
    admission = "NoGo" if regressions else ("IndependentReviewRequired" if not uncertain and primary["simultaneous_95"][1] < 1 else "Inconclusive")
    write(out/"summary.json",dict(status="Pass",scope="all registered costs computed; production admission separate",production_admission={"R":admission},
        samples_sha256=sha(out/"samples.json"),processes_sampled=2508,requests=10032,warmup_processes=76,retained_processes=2432,
        method=dict(statistic="median of within-round R/BASE ratios",rounds=64,family=95,alpha=.05,k=k,
                    order_statistics=[18,47],per_comparison_tail_probability=2*sum(math.comb(64,j) for j in range(k+1))/2**64,margin=MARGIN),
        primary=short(primary),cells=cells,processes=processes,regressions=regressions,
        cannot_exclude_one_percent_regression=uncertain,point_estimates_above_one_percent=[short(r) for r in all_cells if r["paired_median"]>MARGIN],
        limitations=["Nominal simultaneous intervals assume stable sufficiently independent paired rounds; report order and drift",
            "1% margin and all 95 endpoints fixed before samples; no expansion, deletion, or post-hoc pooling",
            "Cold means empty compiler session, not flushed OS caches; parent VmHWM excludes clang child peak",
            "Original append-only edits remain; six additional real consumer cells edit and restore executed bodies",
            "No inference of general program complexity or Rust product advantage from this bounded experiment"]))
    print("registered performance decision:",admission,"explicit regressions:",len(regressions),"uncertain upper bounds:",len(uncertain),flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation",required=True)
    q = sub.add_parser("prepare")
    q.add_argument("--repo",type=Path,required=True);q.add_argument("--candidate",required=True);q.add_argument("--harness",required=True);q.add_argument("--out",type=Path,required=True)
    q = sub.add_parser("sample");q.add_argument("--out",type=Path,required=True);q.add_argument("--window",required=True)
    for name in ["functional","verify","analyze"]:
        q = sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args = parser.parse_args()
    if args.operation == "prepare":prepare(args)
    elif args.operation == "sample":sample(args)
    else:globals()[args.operation](args.out.resolve())

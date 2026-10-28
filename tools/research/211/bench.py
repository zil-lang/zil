#!/usr/bin/env python3
"""211: allocation/lifetime candidate versus 210 direct Builder at identical CGU/job settings; standard-library-only, append-only data.

prepare freezes actual binaries, source fixtures, weights, CPU allocation and order.
preflight checks every source version. measure requires that preflight and preserves
all failures. This runner never pools observations with the historical Rust tests.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
REQUESTS = {"cold_session": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}
SNAPSHOTS = {"cold_session": "base", "unchanged": "base", "private_edit": "private", "public_edit": "public"}


def sha(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for data in iter(lambda: f.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def write(path: Path, obj: object) -> None:
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n")


def cmdout(args: list[str]) -> str:
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT).strip()


def fresh(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=False)
    return path


def prepare(args: argparse.Namespace) -> None:
    out = fresh(args.out.resolve())
    module_path = ROOT / "tools/research/210/corpus.py"
    spec = importlib.util.spec_from_file_location("corpus203", module_path)
    corpus = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(corpus)
    cases = corpus.generate(out)
    available = sorted(os.sched_getaffinity(0))
    if len(available) < 4:
        raise SystemExit("This frozen 4-CPU experiment requires at least four allowed CPUs")
    cpus = available[:4]
    binaries = {"A": args.baseline.resolve(), "B": args.candidate.resolve(), "clang": args.clang.resolve()}
    for binary in binaries.values():
        if not binary.is_file():
            raise FileNotFoundError(binary)
    order = []
    rng = random.Random(211)
    for block in range(5):
        ids = [c["id"] for c in cases]
        rng.shuffle(ids)
        for case in ids:
            for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"):
                order.append({"block": block, "case": case, "position": position, "arm": arm})
    files = {str(p.relative_to(out)): sha(p) for p in sorted((out / "corpus").rglob("*")) if p.is_file()}
    plan = {"schema": "zil-native-perf211/1", "profile": "O0", "blocks": 5,
            "created_unix": time.time(), "source_commit": cmdout(["git", "-C", str(ROOT), "rev-parse", "HEAD"]),
            "source_dirty": cmdout(["git", "-C", str(ROOT), "status", "--porcelain"]),
            "runner_sha256": sha(Path(__file__)), "binaries": {k: {"path": str(v), "sha256": sha(v)} for k,v in binaries.items()},
            "clang_version": cmdout([str(binaries["clang"]), "--version"]),
            "cpus": cpus, "cpu_max": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
            "host_shared": True, "cases": cases, "requests": REQUESTS, "order": order,
            "flags": {"A": ["--codegen-units", "8", "--codegen-jobs", "4"], "B": ["--codegen-units", "8", "--codegen-jobs", "4"]},
            "environment": {key: os.environ.get(key, "") for key in ["PATH", "LD_LIBRARY_PATH", "ZIL_CLANG"]},
            "fixtures_sha256": files, "expected_builds": len(order)*len(REQUESTS),
            "scope": "direct compiler invocations, four allowed CPUs, no object cache, not build-system no-op"}
    if plan["source_dirty"]:
        raise SystemExit("Source must be committed before freezing a measured plan")
    write(out / "plan.json", plan)
    print(out / "plan.json", flush=True)


def load(plan_path: Path) -> tuple[dict, Path]:
    plan_path = plan_path.resolve()
    plan = json.loads(plan_path.read_text())
    root = plan_path.parent
    if sha(Path(__file__)) != plan["runner_sha256"]:
        raise SystemExit("Runner changed after freeze; create a new plan, retain old evidence")
    for item in plan["binaries"].values():
        if sha(Path(item["path"])) != item["sha256"]:
            raise SystemExit("Measured tool changed after freeze")
    for path, expected in plan["fixtures_sha256"].items():
        if sha(root/path) != expected:
            raise SystemExit(f"Frozen fixture changed: {path}")
    if any(os.environ.get(k, "") != v for k,v in plan["environment"].items()):
        raise SystemExit("Frozen tool environment changed")
    return plan, root


def sync_sources(src: Path, dest: Path) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    current = {p.name for p in src.iterdir() if p.is_file()}
    for path in dest.iterdir():
        if path.is_file() and path.name not in current:
            path.unlink()
    for path in src.iterdir():
        if path.is_file():
            target = dest/path.name
            data = path.read_bytes()
            if not target.exists() or target.read_bytes() != data:
                target.write_bytes(data)


def one_build(plan: dict, root: Path, case: dict, arm: str, request: str,
              work: Path, logroot: Path, tag: str, profile: str = "O0",
              flags: list[str] | None = None, cpus: list[int] | None = None) -> dict:
    snapshot = SNAPSHOTS[request]
    sync_sources(root/case["root"]/snapshot/"zil", work/"src")
    out = work/"program"
    for path in [out, out.with_name(out.name+".ll")]:
        path.unlink(missing_ok=True)
    argv = [plan["binaries"][arm]["path"], "build", str(work/"src/main.zil"),
            "-o", str(out), "--cache-dir", str(work/"cache"), "--clang", plan["binaries"]["clang"]["path"],
            "--opt", profile, "--json", *(plan["flags"][arm] if flags is None else flags)]
    stdout_path, stderr_path = logroot/f"{tag}.stdout.json", logroot/f"{tag}.stderr.txt"
    def affinity() -> None:
        os.sched_setaffinity(0, set(plan["cpus"] if cpus is None else cpus))
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        start = time.perf_counter_ns()
        process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, preexec_fn=affinity)
        _, status, usage = os.wait4(process.pid, 0)
        stop = time.perf_counter_ns()
        process.returncode = os.waitstatus_to_exitcode(status)
    record = {"argv": argv, "wall_seconds": (stop-start)/1e9, "exit": process.returncode,
              "cpu_user": usage.ru_utime, "cpu_system": usage.ru_stime, "maxrss_kib": usage.ru_maxrss,
              "case": case["id"], "arm": arm, "request": request, "profile": profile,
              "stdout": stdout_path.name, "stderr": stderr_path.name, "checks": []}
    try:
        report = json.loads(stdout_path.read_text())
        record.update(status=report.get("status"), verification_runs=report.get("verification_runs"),
                      native_codegen=report.get("native_codegen"), commands=report.get("commands", []),
                      disk_cache=report.get("disk_cache"))
    except (ValueError, OSError) as error:
        record["parse_error"] = repr(error)
    if record["exit"] == 0 and record.get("status") == "Success" and out.is_file():
        for item in case["inputs"]:
            observed = subprocess.run([str(out)], input=(root/item["path"]).read_bytes(), capture_output=True,
                                      timeout=60, preexec_fn=affinity)
            expected = (root/item["expected"][snapshot]).read_bytes()
            record["checks"].append({"input_bytes": item["bytes"], "exit": observed.returncode,
                "matches": observed.returncode == 0 and observed.stdout == expected and not observed.stderr,
                "stdout_sha256": hashlib.sha256(observed.stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(observed.stderr).hexdigest()})
        record["executable_sha256"] = sha(out)
        record["executable_bytes"] = out.stat().st_size
        ir = out.with_name(out.name+".ll")
        record["llvm_bytes"] = ir.stat().st_size
        record["llvm_sha256"] = sha(ir)
    record["pass"] = (record["exit"] == 0 and record.get("verification_runs") == 1 and
                      len(record["checks"]) == len(case["inputs"]) and all(c["matches"] for c in record["checks"]))
    return record


def preflight(args: argparse.Namespace) -> None:
    plan, root = load(args.plan)
    out = fresh(args.out.resolve()); logroot = fresh(out/"logs")
    records = []
    with (out/"records.jsonl").open("x") as log:
        for case in plan["cases"]:
            for arm in "AB":
                work = fresh(out/f"{case['id']}-{arm}")
                for request in ["cold_session", "private_edit", "public_edit"]:
                    tag=f"{case['id']}-{arm}-{request}"
                    rec=one_build(plan,root,case,arm,request,work,logroot,tag)
                    records.append(rec); log.write(json.dumps(rec)+"\n");log.flush()
                    print(tag,rec["pass"],rec.get("native_codegen"),flush=True)
    write(out/"result.json", {"plan_sha256":sha(args.plan),"builds":len(records),"pass":all(r["pass"] for r in records),"failures":sum(not r["pass"] for r in records)})


def measure(args: argparse.Namespace) -> None:
    plan, root = load(args.plan)
    preflight = json.loads(args.preflight.read_text())
    if preflight.get("plan_sha256") != sha(args.plan) or not preflight.get("pass") or not args.quiet_window:
        raise SystemExit("Complete matching preflight and explicit quiet window required")
    out = fresh(args.out.resolve()); logs = fresh(out/"logs")
    byid={c["id"]:c for c in plan["cases"]}
    with (out/"records.jsonl").open("x") as log:
        index=0
        for seq in plan["order"]:
            work=fresh(out/f"b{seq['block']}-{seq['case']}-p{seq['position']}-{seq['arm']}")
            for request in REQUESTS:
                tag=f"{index:04d}"
                rec=one_build(plan,root,byid[seq["case"]],seq["arm"],request,work,logs,tag)
                rec.update(block=seq["block"],position=seq["position"],index=index)
                log.write(json.dumps(rec)+"\n");log.flush();index+=1
                print(index,plan["expected_builds"],seq["case"],seq["arm"],request,round(rec["wall_seconds"],4),rec["pass"],flush=True)
    write(out/"complete.json",{"plan_sha256":sha(args.plan),"rows":index,"records_sha256":sha(out/"records.jsonl")})


def aggregate(plan: dict, rows: list[dict], blocks: list[int], metric="wall_seconds") -> dict:
    cells={}
    for row in rows:
        cells.setdefault((row["block"],row["case"],row["request"],row["arm"]),[]).append(row[metric])
    totals={"A":0.0,"B":0.0}; groups={}; requests={r:{"A":0.0,"B":0.0} for r in REQUESTS}
    for case in plan["cases"]:
        group={"A":0.0,"B":0.0}
        for request, weight in REQUESTS.items():
            for arm in "AB":
                value=statistics.median(statistics.mean(cells[(block,case["id"],request,arm)]) for block in blocks)
                group[arm]+=value*weight
                requests[request][arm]+=case["weight"]*value
                totals[arm]+=case["weight"]*weight*value
        group["B_over_A"]=group["B"]/group["A"]
        groups[case["id"]]=group
    for req in requests.values(): req["B_over_A"]=req["B"]/req["A"]
    return {"A":totals["A"],"B":totals["B"],"B_over_A":totals["B"]/totals["A"],"projects":groups,"requests":requests}


def summarize(args: argparse.Namespace) -> None:
    plan,_=load(args.plan)
    rows=[json.loads(line) for line in args.records.read_text().splitlines()]
    required={(b,c["id"],r,a):2 for b in range(plan["blocks"]) for c in plan["cases"] for r in REQUESTS for a in "AB"}
    actual={}
    for row in rows:
        key=(row["block"],row["case"],row["request"],row["arm"])
        actual[key]=actual.get(key,0)+1
    complete=actual==required and len(rows)==plan["expected_builds"] and all(r["pass"] for r in rows)
    result={"schema":"zil-native-perf210-summary/1","plan_sha256":sha(args.plan),"records_sha256":sha(args.records),
            "rows":len(rows),"expected":plan["expected_builds"],"failures":sum(not r["pass"] for r in rows),
            "complete":complete,"result":None,"confidence":"exploratory, three paired blocks on shared host; not product gate"}
    if complete:
        result["result"]=aggregate(plan,rows,list(range(plan["blocks"])))
        for r in rows: r["cpu_seconds"]=r["cpu_user"]+r["cpu_system"]
        result["cpu_auxiliary"]=aggregate(plan,rows,list(range(plan["blocks"])),"cpu_seconds")
        rng=random.Random(211)
        boot=sorted(aggregate(plan,rows,[rng.randrange(plan["blocks"]) for _ in range(plan["blocks"])])["B_over_A"] for _ in range(2000))
        result["exploratory_ci95"]=[boot[50],boot[1949]]
        result["block_results"]=[aggregate(plan,rows,[b]) for b in range(plan["blocks"])]
    write(args.out,result)
    print(json.dumps(result,ensure_ascii=False,indent=2))


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="action",required=True)
    p=sub.add_parser("prepare")
    for name in ["out","baseline","candidate","clang"]:p.add_argument("--"+name,type=Path,required=True)
    p=sub.add_parser("preflight")
    for name in ["plan","out"]:p.add_argument("--"+name,type=Path,required=True)
    p=sub.add_parser("measure")
    for name in ["plan","preflight","out"]:p.add_argument("--"+name,type=Path,required=True)
    p.add_argument("--quiet-window",action="store_true")
    p=sub.add_parser("summarize")
    for name in ["plan","records","out"]:p.add_argument("--"+name,type=Path,required=True)
    args=parser.parse_args();globals()[args.action](args)

if __name__=="__main__":main()

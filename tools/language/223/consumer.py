#!/usr/bin/env python3
"""Three-module Product Store consumer with a frozen source-derived oracle.

The actual ZIL source owns four independent ByteBuffers and four Marker
resources beneath two nested Product elements. The observer only wraps the
real realloc/free ABI; it neither implements Product nor supplies cleanup.
Linux x86-64 natural layouts give Leaf=32, Pair=72, Root=80 bytes. Therefore
the two Store capacities request 160 and 320 bytes. All expected outputs,
allocation counts and failure cleanup are frozen before compiling/running.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def command(argv, directory, data=b"", environment=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"command": [str(x) for x in argv], "status": "Running", "start": time.time(),
               "stdin_hex": data.hex(), "controlled_environment": environment or {}}
    save(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], input=data, capture_output=True, timeout=120,
                                env=dict(os.environ, **(environment or {})))
        (directory / "stdout.log").write_bytes(result.stdout)
        (directory / "stderr.log").write_bytes(result.stderr)
        receipt.update(status="Completed", exit_code=result.returncode,
                       stdout_sha256=digest(directory / "stdout.log"),
                       stderr_sha256=digest(directory / "stderr.log"))
        return result
    except subprocess.TimeoutExpired as error:
        (directory / "stdout.log").write_bytes(error.stdout or b"")
        (directory / "stderr.log").write_bytes(error.stderr or b"")
        receipt.update(status="Timeout", exit_code=None)
        raise
    finally:
        receipt["end"] = time.time()
        save(directory / "command.json", receipt)


def expected(mode, fail_at):
    """Oracle follows source transfer/drop order; no IR/results are read."""
    allocation = {"alloc_calls": 0, "allocated": 0, "frees": 0, "failures": 0,
                  "live": 0, "peak_live": 0, "peak_bytes": 0}
    if mode not in ("0", "1", "2", "3"):
        return {"stdout": "", "exit": 9, "allocation": allocation}
    # (request, previous allocation size). The last call grows the same Store.
    requests = [(160, 0), (2, 0), (3, 0)]
    if mode != "0":
        requests.extend([(4, 0), (5, 0)])
    if mode == "3":
        requests.append((320, 160))
    live_bytes = 0
    for index, (requested, previous) in enumerate(requests, 1):
        allocation["alloc_calls"] += 1
        if index == fail_at:
            allocation["failures"] = 1
            break
        if previous == 0:
            allocation["allocated"] += 1
        live_bytes += requested - previous
        allocation["peak_live"] = max(allocation["peak_live"], allocation["allocated"])
        allocation["peak_bytes"] = max(allocation["peak_bytes"], live_bytes)
    allocation["frees"] = allocation["allocated"]
    if allocation["failures"]:
        # Each failing leaf destroys its own marker first; the already-built
        # sibling follows, then complete elements in the surviving Store prefix.
        output = {1: "", 2: "A", 3: "BA", 4: "CAB", 5: "DCAB", 6: "ABCD"}[fail_at]
    else:
        output = "AB" if mode == "0" else "CDAB" if mode == "2" else "ABCD"
    return {"stdout": output, "exit": 1 if allocation["failures"] else 0,
            "allocation": allocation}


def scenarios():
    rows = []
    for mode in ("0", "1", "2", "3", "x", ""):
        ordinary = expected(mode, 0)
        calls = ordinary["allocation"]["alloc_calls"]
        probes = [{"name": "allocation", "fail_at": 0, **ordinary}]
        for failure in range(1, calls + 1):
            probes.append({"name": f"failure-{failure}", "fail_at": failure,
                           **expected(mode, failure)})
        probes.append({"name": "after-last", "fail_at": calls + 1, **ordinary})
        rows.append({"name": "eof" if not mode else "input-" + mode,
                     "input": mode, "stdout": ordinary["stdout"], "exit": ordinary["exit"],
                     "probes": probes})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-build-receipt", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("the frozen byte-layout oracle is explicitly Linux x86-64")
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
    if dirty:
        (out / "dirty.txt").write_text(dirty)
        raise RuntimeError("consumer acceptance requires a clean committed checkout")
    build_receipt = json.loads(args.compiler_build_receipt.read_text())
    if (build_receipt.get("status") != "Pass" or build_receipt.get("exit_code") != 0
            or build_receipt.get("git_status") != "" or build_receipt.get("cli_sha256") != digest(cli)):
        raise RuntimeError("compiler identity requires a successful clean build receipt and exact CLI hash")
    compiler_commit = build_receipt["source_commit"]
    production = ["Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
                  ":(glob)crates/*/Cargo.toml", ":(glob)crates/*/build.rs", ":(glob)crates/*/src/**"]
    difference = subprocess.check_output(["git", "-C", str(repo), "diff", "--name-only", compiler_commit, "HEAD", "--", *production], text=True)
    if difference:
        (out / "compiler-difference.txt").write_text(difference)
        raise RuntimeError("compiler production inputs differ from this checkout")
    sources = repo / "examples/mainline/product_store223"
    report = {"status": "Running", "source_commit": commit, "compiler_source_commit": compiler_commit,
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt), "cli_sha256": digest(cli),
              "sources": {p.name: digest(p) for p in sorted(sources.glob("*.zil"))},
              "design_sha256": digest(repo / "_zil_design.md"),
              "allocator_sources": {str(p.relative_to(repo)): digest(p) for p in
                                    [repo / "tools/language/173/allocator_probe.c", repo / "tools/research/142/allocator_probe.c"]},
              "oracles": scenarios(), "builds": [], "observations": [],
              "layout_oracle": {"Leaf": 32, "Pair": 72, "Root": 80, "Store_capacity_2": 160, "Store_capacity_4": 320},
              "platform": "Linux x86-64 native", "full_language": "NotSatisfied", "product": "NotRun"}
    # Freeze all expected outcomes and source identities before build/run.
    save(out / "summary.json", report)
    clang = os.environ.get("ZIL_CLANG", "clang")
    try:
        frozen = out / "sources"
        frozen.mkdir()
        for name in ("main.zil", "model.zil", "storage.zil"):
            (frozen / name).write_bytes((sources / name).read_bytes())
        for opt in ("O0", "O2"):
            directory = out / opt
            executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
            result = command([cli, "build", frozen / "main.zil", "--backend", "llvm-api", "--opt", opt,
                              "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
            compiled = json.loads(result.stdout)
            if (result.returncode or compiled["status"] != "Success" or compiled["verification_runs"] != 1
                    or len(compiled["source_files"]) != 3):
                raise RuntimeError(f"{opt}: {compiled}")
            build = {"optimization": opt, "verification_runs": 1, "module_count": 3,
                     "llvm_sha256": digest(phases / "llvm.ll"), "executable_sha256": digest(executable),
                     "native_codegen": compiled["native_codegen"]}
            report["builds"].append(build)
            save(out / "summary.json", report)
            linked = command([clang, "-" + opt, phases / "llvm.ll", repo / "tools/language/173/allocator_probe.c",
                              "-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt}: allocator observer failed to link")
            build["allocation_probe_sha256"] = digest(probe)
            for row in report["oracles"]:
                cases = [("ordinary", executable, row, {})]
                cases.extend((p["name"], probe, p, {"ZIL_TEST_FAIL_AT": str(p["fail_at"])}) for p in row["probes"])
                for name, program, oracle, environment in cases:
                    result = command([program], directory / row["name"] / name, row["input"].encode(), environment)
                    if result.returncode != oracle["exit"] or result.stdout != oracle["stdout"].encode():
                        raise RuntimeError(f"{opt}/{row['name']}/{name}: exit {result.returncode}, stdout {result.stdout!r}, expected {oracle}")
                    observed = {"optimization": opt, "scenario": row["name"], "mode": name,
                                "input": row["input"], "stdout": result.stdout.decode(), "exit": result.returncode,
                                "fail_at": oracle.get("fail_at", 0)}
                    if "allocation" in oracle:
                        allocation = json.loads(result.stderr)
                        if allocation != oracle["allocation"]:
                            raise RuntimeError(f"{opt}/{row['name']}/{name}: allocation {allocation}, expected {oracle['allocation']}")
                        observed["allocation"] = allocation
                    elif result.stderr:
                        raise RuntimeError(f"unexpected ordinary stderr: {result.stderr!r}")
                    report["observations"].append(observed)
                    save(out / "summary.json", report)
        report.update(status="Pass", native_executions=len(report["observations"]),
                      actual_allocation_failures=sum(r.get("allocation", {}).get("failures", 0) for r in report["observations"]))
    except BaseException as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ("status", "source_commit", "native_executions", "actual_allocation_failures")}, indent=2))


if __name__ == "__main__":
    main()

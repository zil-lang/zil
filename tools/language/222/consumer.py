#!/usr/bin/env python3
"""Source #for O0/O2 consumer with real allocations and receiving-scope cleanup."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def command(argv, directory, data=b"", environment=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"command": [str(value) for value in argv], "status": "Running",
               "start": time.time(), "stdin_hex": data.hex(),
               "controlled_environment": environment or {}}
    save(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], input=data, capture_output=True,
                                timeout=120, env=dict(os.environ, **(environment or {})))
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
    """Frozen source oracle: three independent buffers live to receiving exit."""
    allocation = {"alloc_calls": 0, "allocated": 0, "frees": 0, "failures": 0,
                  "live": 0, "peak_live": 0, "peak_bytes": 0}
    if mode not in ("0", "1", "2", "3"):
        return {"stdout": "", "exit": 9, "allocation": allocation}
    completed = 0
    created = 0
    failed = False
    maximum = min(int(mode) + 1, 3)
    for index in range(maximum):
        created += 1
        allocation["alloc_calls"] += 1
        if fail_at == allocation["alloc_calls"]:
            allocation["failures"] = 1
            failed = True
            break
        completed += 1
        allocation["allocated"] += 1
        allocation["peak_live"] = completed
        allocation["peak_bytes"] += index + 1
    allocation["frees"] = allocation["allocated"]
    output = "abc"[:completed]
    if mode == "3" and not failed:
        output += "Z"
    output += "ABC"[:created][::-1] + "R"
    return {"stdout": output, "exit": 1 if failed else (int(mode) if mode != "3" else 0),
            "allocation": allocation}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-build-receipt", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
    if dirty:
        (out / "dirty.txt").write_text(dirty)
        raise RuntimeError("consumer requires clean committed source")
    receipt = json.loads(args.compiler_build_receipt.read_text())
    if (receipt.get("status") != "Pass" or receipt.get("exit_code") != 0
            or receipt.get("git_status") != "" or receipt.get("cli_sha256") != digest(cli)):
        raise RuntimeError("CLI must match a successful clean compiler build receipt")
    compiler_commit = receipt["source_commit"]
    production = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                  ":(glob)crates/*/Cargo.toml", ":(glob)crates/*/build.rs", ":(glob)crates/*/src/**"]
    if subprocess.check_output(["git", "-C", str(repo), "diff", "--name-only",
                                compiler_commit, "HEAD", "--", *production], text=True):
        raise RuntimeError("compiler production source changed after its clean build")
    source_dir = repo / "examples/mainline/ct_for222"
    frozen = out / "sources"
    frozen.mkdir()
    for path in sorted(source_dir.glob("*.zil")):
        (frozen / path.name).write_bytes(path.read_bytes())
    oracles = []
    for mode in ("0", "1", "2", "3", "x", ""):
        ordinary = expected(mode, 0)
        controls = range(ordinary["allocation"]["alloc_calls"] + 2)
        oracles.append({"input": mode, "name": mode or "eof", "ordinary": ordinary,
                        "probes": [{"fail_at": fail_at, **expected(mode, fail_at)} for fail_at in controls]})
    result = {"status": "Running", "source_commit": commit,
              "compiler_source_commit": compiler_commit, "cli_sha256": digest(cli),
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt),
              "observer_source_sha256": digest(repo / "tools/language/173/allocator_probe.c"),
              "consumer_script_sha256": digest(Path(__file__).resolve()),
              "sources": {path.name: digest(path) for path in sorted(frozen.glob("*.zil"))},
              "design_sha256": digest(repo / "_zil_design.md"),
              "oracles": oracles, "builds": [], "observations": [],
              "platform": "Linux native", "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", result)
    clang = os.environ.get("ZIL_CLANG", "clang")
    try:
        for opt in ("O0", "O2"):
            directory = out / opt
            executable, probe, phases = directory / "program", directory / "probe", directory / "phases"
            built = command([cli, "build", frozen / "main.zil", "--backend", "llvm-api", "--opt", opt,
                             "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
            report = json.loads(built.stdout)
            if built.returncode or report["status"] != "Success" or report["verification_runs"] != 1:
                raise RuntimeError(f"{opt} build failed: {report}")
            if len(report["source_files"]) != 3:
                raise RuntimeError("consumer must use its three actual source modules")
            trace = json.loads((phases / "selection.json").read_text())
            if len(trace["generations"]) != 1 or trace["generations"][0]["iterations"] != 3:
                raise RuntimeError("source generation did not perform exactly three iterations")
            dependencies = {item["function"] for item in trace["generations"][0]["dependencies"]}
            if dependencies != {"rounds", "configured_rounds"}:
                raise RuntimeError(f"generation dependency trace differs: {dependencies}")
            linked = command([clang, "-" + opt, phases / "llvm.ll", repo / "tools/language/173/allocator_probe.c",
                              "-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt} allocator observer linking failed")
            result["builds"].append({"optimization": opt, "native_codegen": report["native_codegen"],
                                     "llvm_sha256": digest(phases / "llvm.ll"),
                                     "executable_sha256": digest(executable), "probe_sha256": digest(probe),
                                     "selection": trace})
            for row in oracles:
                cases = [("ordinary", executable, row["ordinary"], {})]
                cases += [(f"fail-{control['fail_at']}", probe, control,
                           {"ZIL_TEST_FAIL_AT": str(control["fail_at"])}) for control in row["probes"]]
                for name, program, oracle, environment in cases:
                    actual = command([program], directory / row["name"] / name,
                                     row["input"].encode(), environment)
                    if actual.returncode != oracle["exit"] or actual.stdout != oracle["stdout"].encode():
                        raise RuntimeError(f"{opt}/{row['name']}/{name}: actual {actual.returncode}/{actual.stdout!r}, oracle {oracle}")
                    observed = {"optimization": opt, "scenario": row["name"], "mode": name,
                                "stdout": actual.stdout.decode(), "exit": actual.returncode,
                                "fail_at": oracle.get("fail_at", 0)}
                    if name != "ordinary":
                        allocation = json.loads(actual.stderr)
                        if allocation != oracle["allocation"]:
                            raise RuntimeError(f"{opt}/{row['name']}/{name}: allocation {allocation}, oracle {oracle['allocation']}")
                        observed["allocation"] = allocation
                    elif actual.stderr:
                        raise RuntimeError(f"ordinary execution wrote unexpected stderr: {actual.stderr!r}")
                    result["observations"].append(observed)
                    save(out / "summary.json", result)
        result.update(status="Pass", native_executions=len(result["observations"]),
                      actual_allocation_failures=sum(row.get("allocation", {}).get("failures", 0) for row in result["observations"]))
    except BaseException as error:
        result.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", result)
        raise
    save(out / "summary.json", result)
    print(json.dumps({key: result[key] for key in ("status", "source_commit", "native_executions", "actual_allocation_failures")}, indent=2))


if __name__ == "__main__":
    main()

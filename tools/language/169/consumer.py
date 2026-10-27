#!/usr/bin/env python3
"""Run the actual multi-module 169 consumer and its native cleanup failures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


CASES = {
    "main": {"stdout": b"AACBACB", "exit": 0, "allocations": 4,
             "fail_at": 1, "failed_stdout": b"A"},
    "prepare_failure": {"stdout": b"BA", "exit": 1, "allocations": 2,
                        "fail_at": 2, "failed_stdout": b"BA"},
    "argument_failure": {"stdout": b"BA", "exit": 1, "allocations": 2,
                         "fail_at": 2, "failed_stdout": b"BA"},
    "error_entry": {"stdout": b"D", "exit": 1, "allocations": 1,
                    "fail_at": 1, "failed_stdout": b"D"},
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def command(argv, directory, env=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"command": [str(arg) for arg in argv], "start": time.time(), "status": "Running"}
    if env is not None:
        receipt["controlled_environment"] = env
    save(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], capture_output=True, timeout=120,
                                env=None if env is None else dict(os.environ, **env))
        (directory / "stdout.log").write_bytes(result.stdout)
        (directory / "stderr.log").write_bytes(result.stderr)
        receipt.update(status="Completed", exit_code=result.returncode,
                       stdout_sha256=digest(directory / "stdout.log"),
                       stderr_sha256=digest(directory / "stderr.log"))
    except subprocess.TimeoutExpired as error:
        receipt.update(status="Timeout", exit_code=None)
        (directory / "stdout.log").write_bytes(error.stdout or b"")
        (directory / "stderr.log").write_bytes(error.stderr or b"")
        raise
    finally:
        receipt["end"] = time.time()
        save(directory / "command.json", receipt)
    return result


def native_result(result, expected, exit_code):
    if result.returncode != exit_code or result.stdout != expected:
        raise RuntimeError(f"native observation differs: exit={result.returncode}, stdout={result.stdout!r}, stderr={result.stderr!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--source-commit", help="exact source identity for an evidence-free unpacked source tree")
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source_commit = args.source_commit or subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    sources = repo / "examples/mainline/zui_language169"
    report = {"status": "Running", "source_commit": source_commit, "cli_sha256": digest(cli),
              "sources": {path.name: digest(path) for path in sorted(sources.glob("*.zil"))},
              "observations": [], "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", report)
    try:
        for name, case in CASES.items():
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
                build = command([cli, "build", sources / (name + ".zil"), "--opt", opt,
                                 "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
                compilation = json.loads(build.stdout)
                if build.returncode or compilation["status"] != "Success" or compilation["verification_runs"] != 1:
                    raise RuntimeError(f"{name} {opt} build: {compilation}")
                ordinary = command([executable], directory / "ordinary")
                native_result(ordinary, case["stdout"], case["exit"])
                linked = command(["clang", "-" + opt, phases / "llvm.ll",
                                  repo / "tools/research/142/allocator_probe.c", "-Wl,--wrap=realloc",
                                  "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                if linked.returncode:
                    raise RuntimeError(f"{name} {opt}: allocator probe link failed")
                for fail in (False, True):
                    env = {"ZIL_TEST_FAIL_AT": str(case["fail_at"] if fail else 0)}
                    observed = command([probe], directory / ("allocation-failure" if fail else "allocation"), env)
                    native_result(observed, case["failed_stdout"] if fail else case["stdout"],
                                  1 if fail else case["exit"])
                    allocation = json.loads(observed.stderr)
                    if (allocation["live"] != 0 or allocation["allocated"] != allocation["frees"]
                            or allocation["failures"] != int(fail)):
                        raise RuntimeError(f"{name} {opt}: allocation/cleanup imbalance: {allocation}")
                    if not fail and allocation["allocated"] != case["allocations"]:
                        raise RuntimeError(f"{name} {opt}: unexpected actual allocation count: {allocation}")
                    report["observations"].append({"case": name, "optimization": opt,
                                                   "allocation_failure": fail, "exit_code": observed.returncode,
                                                   "stdout": observed.stdout.decode(), "allocation": allocation,
                                                   "llvm_sha256": digest(phases / "llvm.ll")})
                    save(out / "summary.json", report)
        report.update(status="Pass", ordinary_executions=8, instrumented_executions=16,
                      scope="Finite owned result/array/closure composition and native cleanup; not full ZUI readiness")
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

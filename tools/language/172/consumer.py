#!/usr/bin/env python3
"""Exercise actual ZIL owning prefixes and every native allocation failure point."""
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
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def command(argv, directory, input_data=b"", env=None):
    directory.mkdir(parents=True, exist_ok=False)
    (directory / "stdin").write_bytes(input_data)
    receipt = {"command": [str(arg) for arg in argv], "start": time.time(),
               "status": "Running", "stdin_sha256": digest(directory / "stdin")}
    if env is not None:
        receipt["controlled_environment"] = env
    save(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], input=input_data, capture_output=True,
                                timeout=120, env=dict(os.environ, **(env or {})))
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


def expect_native(result, expected, exit_code):
    if result.returncode != exit_code or result.stdout != expected:
        raise RuntimeError(f"native observation differs: exit={result.returncode}, "
                           f"stdout={result.stdout!r}, stderr={result.stderr!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--source-commit")
    parser.add_argument("--compiler-commit")
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source_commit = args.source_commit or subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    sources = repo / "examples/mainline/impl172"
    report = {"status": "Running", "source_commit": source_commit,
              "compiler_source_commit": args.compiler_commit or source_commit,
              "cli_sha256": digest(cli),
              "sources": {path.name: digest(path) for path in sorted(sources.glob("*.zil"))},
              "probe_sha256": digest(repo / "tools/research/142/allocator_probe.c"),
              "observations": [], "ordinary_executions": 0,
              "instrumented_executions": 0, "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", report)
    try:
        for name in ("main", "reserve_recovery", "full_push"):
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
                build = command([cli, "build", sources / (name + ".zil"), "--opt", opt,
                                 "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
                compilation = json.loads(build.stdout)
                if build.returncode or compilation["status"] != "Success" or compilation["verification_runs"] != 1:
                    raise RuntimeError(f"{name} {opt} build: {compilation}")
                ordinary_cases = [(b"", b"BA", 0)] if name == "reserve_recovery" else [(b"", b"Z", 1)]
                if name == "main":
                    ordinary_cases = [(b"0", b"", 0), (b"1", b"A", 0),
                                      (b"4", b"DCBA", 0), (b"8", b"HGFEDCBA", 0)]
                for number, (data, expected, exit_code) in enumerate(ordinary_cases):
                    observed = command([executable], directory / f"ordinary-{number}", data)
                    expect_native(observed, expected, exit_code)
                    report["ordinary_executions"] += 1
                    report["observations"].append({"case": name, "optimization": opt,
                                                   "instrumented": False, "stdin": data.decode(),
                                                   "exit_code": observed.returncode, "stdout": observed.stdout.decode(),
                                                   "llvm_sha256": digest(phases / "llvm.ll")})
                    save(out / "summary.json", report)
                linked = command(["clang", "-" + opt, phases / "llvm.ll",
                                  repo / "tools/research/142/allocator_probe.c", "-Wl,--wrap=realloc",
                                  "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                if linked.returncode:
                    raise RuntimeError(f"{name} {opt}: allocator probe link failed")
                # Every entry specifies observed prefix ownership: a reserve
                # failure keeps the previous head and elements, and a failed
                # element preparation drops only that temporary before prefix
                # cleanup. The executable remains real LLVM-compiled ZIL.
                if name == "main":
                    cases = [(0, b"DCBA", 0, 6, 5), (1, b"", 1, 1, 0),
                             (2, b"A", 1, 2, 1), (3, b"BA", 1, 3, 2),
                             (4, b"AB", 1, 4, 3), (5, b"CAB", 1, 5, 3),
                             (6, b"DABC", 1, 6, 4)]
                    data = b"4"
                elif name == "reserve_recovery":
                    cases = [(0, b"BA", 0, 4, 3), (4, b"BA", 0, 4, 3)]
                    data = b""
                else:
                    cases = [(0, b"Z", 1, 1, 1), (1, b"Z", 1, 1, 0)]
                    data = b""
                for fail_at, expected, exit_code, calls, allocated in cases:
                    observed = command([probe], directory / f"allocation-{fail_at}", data,
                                       {"ZIL_TEST_FAIL_AT": str(fail_at)})
                    expect_native(observed, expected, exit_code)
                    allocation = json.loads(observed.stderr)
                    wanted = {"alloc_calls": calls, "allocated": allocated, "frees": allocated,
                              "failures": int(fail_at != 0), "live": 0}
                    if any(allocation[key] != value for key, value in wanted.items()):
                        raise RuntimeError(f"{name} {opt} failure {fail_at}: {allocation} != {wanted}")
                    report["instrumented_executions"] += 1
                    report["observations"].append({"case": name, "optimization": opt,
                                                   "instrumented": True, "stdin": data.decode(),
                                                   "failure_index": fail_at, "exit_code": observed.returncode,
                                                   "stdout": observed.stdout.decode(), "allocation": allocation,
                                                   "llvm_sha256": digest(phases / "llvm.ll")})
                    save(out / "summary.json", report)
        report.update(status="Pass", scope="Finite direct-Managed Package prefixes; runtime counts, native allocator failures, reserve recovery, and complete failed-push ownership return")
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

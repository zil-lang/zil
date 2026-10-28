#!/usr/bin/env python3
"""Run real lending environments through cross-module helpers and owning State.

Every allocation fault has a precomputed output and responsibility oracle.
Execution records reuse the already checked command recorder from slice178.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    spec = importlib.util.spec_from_file_location("record178", repo / "tools/language/178/consumer.py")
    common = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(common)
    out.mkdir(parents=True, exist_ok=False)
    sources = repo / "examples/mainline/lending184"
    report = {"status": "Running", "source_commit": subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
        "git_status": subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True),
        "cli_sha256": common.digest(cli),
        "sources": {p.name: common.digest(p) for p in sorted(sources.glob("*.zil"))},
        "builds": [], "observations": [], "full_language": "NotSatisfied", "product": "NotRun"}
    common.save(out / "summary.json", report)

    def observe(executable, directory, count, expected, exit_code, allocation=None, fail_at=0):
        environment = None if allocation is None else {"ZIL_TEST_FAIL_AT": str(fail_at)}
        result = common.command([executable], directory, str(count).encode(), environment)
        record = {"input": count, "stdout": result.stdout.decode(), "exit_code": result.returncode,
                  "expected_stdout": expected.decode(), "expected_exit": exit_code,
                  "fail_at": fail_at, "command": str(directory.relative_to(out) / "command.json")}
        if result.stdout != expected or result.returncode != exit_code:
            raise RuntimeError(f"execution differs from oracle: {record}, stderr={result.stderr!r}")
        if allocation is None:
            if result.stderr:
                raise RuntimeError(f"ordinary stderr: {result.stderr!r}")
        else:
            actual = json.loads(result.stderr)
            record["allocation"] = actual
            expected_calls, expected_fresh = allocation
            if (actual["alloc_calls"] != expected_calls or actual["allocated"] != expected_fresh
                    or actual["frees"] != expected_fresh or actual["live"] != 0
                    or actual["failures"] != int(fail_at > 0)):
                raise RuntimeError(f"allocation oracle mismatch: {record}")
        report["observations"].append(record)
        common.save(out / "summary.json", report)

    try:
        for opt in ("O0", "O2"):
            directory = out / opt
            executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
            built = common.command([cli, "build", sources / "main.zil", "--opt", opt,
                                    "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
            compiled = json.loads(built.stdout)
            if built.returncode or compiled["status"] != "Success" or compiled["verification_runs"] != 1:
                raise RuntimeError(f"{opt} source failed: {compiled}")
            if len(compiled["source_files"]) != 4:
                raise RuntimeError("consumer did not load all four actual modules")
            contracts = [c for i in compiled["scalar_interfaces"] for c in i["callable_returns"]]
            if len(contracts) != 2 or any(not c["lending_captures"] or len(c["cleanup"]) != 2 for c in contracts):
                raise RuntimeError("verified factory interfaces lost their lending or cleanup sources")
            report["builds"].append({"optimization": opt, "verification_runs": 1,
                                      "module_count": 4, "llvm_sha256": common.digest(phases / "llvm.ll"),
                                      "executable_sha256": common.digest(executable),
                                      "callable_returns": contracts})
            linked = common.command(["clang", "-" + opt, phases / "llvm.ll",
                                      repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                                      "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt} allocation shim failed to link")
            for count in (0, 1, 2, 3):
                case = directory / f"count-{count}"
                expected = b"A" + bytes(range(66, 66 + count)) + b"HAZ"
                observe(executable, case / "ordinary", count, expected, 0)
                observe(probe, case / "allocation", count, expected, 0, (1 + count, 1 + int(count > 0)))
                for fail_at in range(1, count + 2):
                    failure_output = b"Z" if fail_at == 1 else b"A" + bytes(range(66, 66 + fail_at - 2)) + b"HZ"
                    fresh = 0 if fail_at == 1 else 1 + int(fail_at > 2)
                    observe(probe, case / f"failure-{fail_at}", count, failure_output, 1,
                            (fail_at, fresh), fail_at)
            observe(executable, directory / "invalid", 9, b"", 9)
        report.update(status="Pass", native_executions=len(report["observations"]),
                      fault_executions=sum(r["fail_at"] > 0 for r in report["observations"]))
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        common.save(out / "summary.json", report)
        raise
    common.save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ("status", "source_commit", "native_executions", "fault_executions")}))


if __name__ == "__main__":
    main()

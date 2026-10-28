#!/usr/bin/env python3
"""Check real loop-control helpers, per-iteration cleanup and allocation failure.

The four ZIL modules execute both a Flow repeat helper and a Finite leaf. A
per-iteration buffer and Drop marker survive until the actual continue/break/
ordinary/error exit. The oracle is fixed before any machine output is read.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess


def oracle(count, fail_at=0):
    output = bytearray()
    calls = allocated = complete = 0
    callback_allocated = observed_allocated = False
    for index in range(count):
        # Every entered iteration owns a new scratch buffer, even skipped or
        # breaking iterations. Its marker is printed on every source exit.
        events = ["scratch"]
        if index not in (1, 4):
            events += ["callback", "observed"]
        for owner in events:
            calls += 1
            if calls == fail_at:
                output.extend(bytes([48 + index]) + b"H")
                return bytes(output), 1, calls, allocated
            if owner == "scratch":
                allocated += 1
            elif owner == "callback" and not callback_allocated:
                allocated += 1
                callback_allocated = True
            elif owner == "observed" and not observed_allocated:
                allocated += 1
                observed_allocated = True
        if index not in (1, 4):
            output.append(65 + complete)
            complete += 1
        output.append(48 + index)
        if index == 4:
            break
    output.extend(b"H")
    if fail_at:
        raise ValueError("requested failure is beyond actual allocation calls")
    return bytes(output), 0, calls, allocated


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
    sources = repo / "examples/mainline/loop_control195"
    report = {
        "status": "Running",
        "source_commit": subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
        "git_status": subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True),
        "cli_sha256": common.digest(cli),
        "sources": {p.name: common.digest(p) for p in sorted(sources.glob("*.zil"))},
        "allocator_sources": {str(p.relative_to(repo)): common.digest(p) for p in [repo / "tools/language/173/allocator_probe.c", repo / "tools/research/142/allocator_probe.c"]},
        "builds": [], "observations": [],
        "full_language": "NotSatisfied", "product": "NotRun",
    }
    common.save(out / "summary.json", report)

    def observe(executable, directory, count, allocation=False, fail_at=0):
        expected, code, calls, allocated = oracle(count, fail_at)
        environment = {"ZIL_TEST_FAIL_AT": str(fail_at)} if allocation else None
        result = common.command([executable], directory, str(count).encode(), environment)
        row = {"input": count, "stdout": result.stdout.decode(), "exit_code": result.returncode,
               "expected_stdout": expected.decode(), "expected_exit": code,
               "fail_at": fail_at, "command": str(directory.relative_to(out) / "command.json")}
        if result.stdout != expected or result.returncode != code:
            raise RuntimeError(f"execution differs from fixed oracle: {row}, stderr={result.stderr!r}")
        if allocation:
            actual = json.loads(result.stderr)
            row["allocation"] = actual
            if (actual["alloc_calls"] != calls or actual["allocated"] != allocated
                    or actual["frees"] != allocated or actual["live"] != 0
                    or actual["failures"] != int(fail_at > 0)):
                raise RuntimeError(f"allocation/cleanup differs: {row}")
        elif result.stderr:
            raise RuntimeError(f"ordinary stderr: {result.stderr!r}")
        report["observations"].append(row)
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
            typed = (phases / "typed.txt").read_text()
            if any(marker not in typed for marker in ("traces: Flow(", "traces: Finite(", "kind: Break", "kind: Continue")):
                raise RuntimeError("actual Flow/Finite contracts and control cleanup are required")
            report["builds"].append({"optimization": opt, "verification_runs": 1, "module_count": 4,
                                      "flow_and_finite_contracts": True, "actual_break_continue_cleanup": True,
                                      "llvm_sha256": common.digest(phases / "llvm.ll"),
                                      "executable_sha256": common.digest(executable)})
            linked = common.command(["clang", "-" + opt, phases / "llvm.ll",
                                      repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                                      "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt} actual allocator shim failed to link")
            for count in (0, 1, 2, 3, 7):
                case = directory / f"count-{count}"
                observe(executable, case / "ordinary", count)
                observe(probe, case / "allocation", count, True)
                for fail_at in range(1, oracle(count)[2] + 1):
                    observe(probe, case / f"failure-{fail_at}", count, True, fail_at)
            invalid = common.command([executable], directory / "invalid", b"9")
            if invalid.returncode != 9 or invalid.stdout or invalid.stderr:
                raise RuntimeError("invalid input did not exit before creating owned state")
            report["observations"].append({"input": 9, "stdout": "", "exit_code": 9,
                                             "expected_stdout": "", "expected_exit": 9,
                                             "fail_at": 0, "command": str((directory / "invalid/command.json").relative_to(out))})
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

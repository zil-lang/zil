#!/usr/bin/env python3
"""Exercise Store-bearing State fields with real runtime batches and failure cleanup.

Runtime input determines replacement prefix length. This is a correctness and cleanup check,
not a GUI, full-language readiness verdict, or compiler performance benchmark.
"""
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


def command(argv, directory, input_bytes=b"", environment=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"command": [str(x) for x in argv], "status": "Running", "start": time.time(),
               "stdin_hex": input_bytes.hex(), "controlled_environment": environment or {}}
    save(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], input=input_bytes, capture_output=True,
                                timeout=120, env=dict(os.environ, **(environment or {})))
        (directory / "stdout.log").write_bytes(result.stdout)
        (directory / "stderr.log").write_bytes(result.stderr)
        receipt.update(status="Completed", exit_code=result.returncode,
                       stdout_sha256=digest(directory / "stdout.log"),
                       stderr_sha256=digest(directory / "stderr.log"))
    except subprocess.TimeoutExpired as error:
        (directory / "stdout.log").write_bytes(error.stdout or b"")
        (directory / "stderr.log").write_bytes(error.stderr or b"")
        receipt.update(status="Timeout", exit_code=None)
        raise
    finally:
        receipt["end"] = time.time()
        save(directory / "command.json", receipt)
    return result


def scenarios(name):
    initial = [(1, b"Z", 1, 0), (2, b"AZ", 1, 1), (3, b"BAZ", 1, 2)]
    if name == "main":
        rows = []
        for count in (0, 1, 3):
            # Existing A/B prefix is retired at replacement; final State then
            # destroys Z followed by its newly committed C..E prefix.
            rows.append({"name": f"count-{count}", "input": str(count).encode(),
                         "stdout": b"ABZ" + bytes(range(67, 67 + count)), "exit": 0,
                         "allocated": 3 + int(count > 0) + count, "failures": []})
        rows[-1]["failures"] = initial + [(4, b"ZAB", 1, 3), (5, b"CZAB", 1, 4),
                                         (6, b"DCZAB", 1, 5), (7, b"ECDZAB", 1, 6)]
        rows.append({"name": "invalid-input", "input": b"9", "stdout": b"", "exit": 9,
                     "allocated": 0, "failures": [], "ordinary_only": True})
        return rows
    if name == "take_exit":
        return [{"name": "taken-field", "input": b"", "stdout": b"ABZ", "exit": 1,
                 "allocated": 3, "failures": initial}]
    if name == "reserve_recovery":
        return [{"name": "reserve", "input": b"", "stdout": b"ZAB", "exit": 0,
                 "allocated": 3, "failures": initial + [(4, b"ZAB", 0, 3)]}]
    raise ValueError(name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source-commit")
    parser.add_argument("--compiler-commit")
    parser.add_argument("--case", action="append", choices=["main", "take_exit", "reserve_recovery"])
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    commit = args.source_commit or subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    sources = repo / "examples/mainline/impl174"
    selected = args.case or ["main", "take_exit", "reserve_recovery"]
    report = {"status": "Running", "source_commit": commit,
              "compiler_source_commit": args.compiler_commit or commit,
              "cli_sha256": digest(cli), "selected": selected,
              "sources": {p.name: digest(p) for p in sorted(sources.glob("*.zil"))},
              "probe_sha256": digest(repo / "tools/language/173/allocator_probe.c"),
              "probe_base_sha256": digest(repo / "tools/research/142/allocator_probe.c"),
              "builds": [], "observations": [], "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", report)

    def observe(result, row, name, opt, mode, expected, exit_code, allocated=None, fail_at=0):
        if result.returncode != exit_code or result.stdout != expected:
            raise RuntimeError(f"{name}/{row['name']}/{opt}/{mode}: "
                               f"exit={result.returncode}, stdout={result.stdout!r}, stderr={result.stderr!r}")
        record = {"entry": name, "scenario": row["name"], "optimization": opt,
                  "mode": mode, "stdin_hex": row["input"].hex(), "stdout": result.stdout.decode(),
                  "exit_code": result.returncode, "fail_at": fail_at}
        if allocated is not None:
            allocation = json.loads(result.stderr)
            if (allocation["live"] != 0 or allocation["allocated"] != allocated
                    or allocation["frees"] != allocated or allocation["failures"] != int(fail_at > 0)):
                raise RuntimeError(f"{name}/{row['name']}/{opt}/{mode}: responsibility imbalance {allocation}")
            record["allocation"] = allocation
        elif result.stderr:
            raise RuntimeError(f"unexpected ordinary stderr: {result.stderr!r}")
        report["observations"].append(record)
        save(out / "summary.json", report)

    try:
        for name in selected:
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
                build = command([cli, "build", sources / (name + ".zil"), "--opt", opt,
                                 "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
                compiled = json.loads(build.stdout)
                if build.returncode or compiled["status"] != "Success" or compiled["verification_runs"] != 1:
                    raise RuntimeError(f"{name}/{opt}: {compiled}")
                report["builds"].append({"entry": name, "optimization": opt, "verification_runs": 1,
                                         "llvm_sha256": digest(phases / "llvm.ll")})
                save(out / "summary.json", report)
                linked = command(["clang", "-" + opt, phases / "llvm.ll",
                                  repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                                  "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                if linked.returncode:
                    raise RuntimeError(f"{name}/{opt}: allocator probe link failed")
                for row in scenarios(name):
                    case = directory / row["name"]
                    ordinary = command([executable], case / "ordinary", row["input"])
                    observe(ordinary, row, name, opt, "ordinary", row["stdout"], row["exit"])
                    if row.get("ordinary_only"):
                        continue
                    normal = command([probe], case / "allocation", row["input"], {"ZIL_TEST_FAIL_AT": "0"})
                    observe(normal, row, name, opt, "allocation", row["stdout"], row["exit"], row["allocated"])
                    for fail_at, output, failed_exit, allocated in row["failures"]:
                        failed = command([probe], case / f"failure-{fail_at}", row["input"],
                                         {"ZIL_TEST_FAIL_AT": str(fail_at)})
                        observe(failed, row, name, opt, "allocation_failure", output, failed_exit, allocated, fail_at)
        report.update(status="Pass", native_executions=len(report["observations"]),
                      ordinary_executions=sum(r["mode"] == "ordinary" for r in report["observations"]),
                      instrumented_executions=sum(r["mode"] != "ordinary" for r in report["observations"]))
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

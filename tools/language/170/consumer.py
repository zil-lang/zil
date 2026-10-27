#!/usr/bin/env python3
"""Exercise complete Package array fields and actual allocation failure cleanup."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess


UTILITY = Path(__file__).resolve().parents[1] / "169" / "consumer.py"
SPEC = importlib.util.spec_from_file_location("consumer169_utilities", UTILITY)
UTIL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UTIL)

CASES = {
    "main": {"stdout": b"CBAZDE", "exit": 0, "allocations": 5,
             "failures": {1: b"AZ", 2: b"BAZ", 3: b"CABZ", 4: b"DAZCB", 5: b"EDAZCB"}},
    "prepare_failure": {"stdout": b"BAZ", "exit": 1, "allocations": 2,
                        "failures": {2: b"BAZ"}},
    "error_entry": {"stdout": b"ZAB", "exit": 1, "allocations": 2,
                    "failures": {2: b"ZAB"}},
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = repo / "examples/mainline/impl170"
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    status = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain", "--", "crates", "examples/mainline/impl170", "tools/language/170"], text=True)
    if status:
        raise RuntimeError("commit the exercised source and runner before validation")
    report = {"status": "Running", "source_commit": commit, "compiler_source_commit": commit,
              "cli_sha256": UTIL.digest(cli), "runner_sha256": UTIL.digest(Path(__file__)),
              "utility_sha256": UTIL.digest(UTILITY),
              "sources": {p.name: UTIL.digest(p) for p in sorted(source.glob("*.zil"))},
              "observations": [], "ordinary_executions": 0, "instrumented_executions": 0,
              "full_language": "NotSatisfied", "product": "NotRun"}
    UTIL.save(out / "summary.json", report)
    try:
        for name, case in CASES.items():
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable, probe, phases = directory / "program", directory / "allocation-probe", directory / "phases"
                built = UTIL.command([cli, "build", source / (name + ".zil"), "--opt", opt,
                                      "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
                compilation = json.loads(built.stdout)
                if built.returncode or compilation["status"] != "Success" or compilation["verification_runs"] != 1:
                    raise RuntimeError(f"{name} {opt}: {compilation}")
                ordinary = UTIL.command([executable], directory / "ordinary")
                UTIL.native_result(ordinary, case["stdout"], case["exit"])
                report["ordinary_executions"] += 1
                linked = UTIL.command(["clang", "-" + opt, phases / "llvm.ll",
                                       repo / "tools/research/142/allocator_probe.c", "-Wl,--wrap=realloc",
                                       "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                if linked.returncode:
                    raise RuntimeError(f"{name} {opt}: allocator probe link failed")
                for fail_at, expected in {0: case["stdout"], **case["failures"]}.items():
                    observed = UTIL.command([probe], directory / f"allocation-{fail_at}",
                                            {"ZIL_TEST_FAIL_AT": str(fail_at)})
                    UTIL.native_result(observed, expected, 1 if fail_at else case["exit"])
                    allocation = json.loads(observed.stderr)
                    expected_allocations = fail_at - 1 if fail_at else case["allocations"]
                    if (allocation["live"] != 0 or allocation["allocated"] != allocation["frees"]
                            or allocation["allocated"] != expected_allocations
                            or allocation["failures"] != int(bool(fail_at))):
                        raise RuntimeError(f"{name} {opt} fail_at={fail_at}: {allocation}")
                    report["instrumented_executions"] += 1
                    report["observations"].append({"case": name, "optimization": opt,
                                                   "fail_at": fail_at, "stdout": observed.stdout.decode(),
                                                   "exit_code": observed.returncode, "allocation": allocation,
                                                   "llvm_sha256": UTIL.digest(phases / "llvm.ll")})
                    UTIL.save(out / "summary.json", report)
        report["status"] = "Pass"
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        UTIL.save(out / "summary.json", report)
        raise
    UTIL.save(out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

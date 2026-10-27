#!/usr/bin/env python3
"""Run actual shared resource-head calls and argument-failure cleanup."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess


UTILITY = Path(__file__).resolve().parents[1] / "169" / "consumer.py"
SPEC = importlib.util.spec_from_file_location("consumer169_utilities", UTILITY)
UTIL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UTIL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = repo / "examples/mainline/impl171"
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    paths = ["crates", "examples/mainline/impl171", "tools/language/171"]
    status = subprocess.check_output(
        ["git", "-C", str(repo), "status", "--porcelain", "--", *paths], text=True)
    if status:
        raise RuntimeError("commit the exercised compiler, source and runner before validation")
    report = {"status": "Running", "source_commit": commit, "compiler_source_commit": commit,
              "cli_sha256": UTIL.digest(cli), "runner_sha256": UTIL.digest(Path(__file__)),
              "utility_sha256": UTIL.digest(UTILITY),
              "sources": {p.name: UTIL.digest(p) for p in sorted(source.glob("*.zil"))},
              "observations": [], "native_executions": 0,
              "full_language": "NotSatisfied", "product": "NotRun"}
    UTIL.save(out / "summary.json", report)
    try:
        for name, expected_exit in [("main", 0), ("prepare_failure", 1)]:
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable, phases = directory / "program", directory / "phases"
                build = UTIL.command([cli, "build", source / (name + ".zil"), "--opt", opt,
                                      "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
                compilation = json.loads(build.stdout)
                if build.returncode or compilation["status"] != "Success" or compilation["verification_runs"] != 1:
                    raise RuntimeError(f"{name} {opt} compilation: {compilation}")
                observed = UTIL.command([executable], directory / "native")
                UTIL.native_result(observed, b"A", expected_exit)
                if observed.stderr:
                    raise RuntimeError(f"{name} {opt}: unexpected native stderr {observed.stderr!r}")
                report["native_executions"] += 1
                report["observations"].append({"case": name, "optimization": opt,
                                               "stdout": observed.stdout.decode(),
                                               "exit_code": observed.returncode,
                                               "input_fingerprint": compilation["input_fingerprint"],
                                               "llvm_sha256": UTIL.digest(phases / "llvm.ll")})
                UTIL.save(out / "summary.json", report)
        ending_commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
        ending_status = subprocess.check_output(
            ["git", "-C", str(repo), "status", "--porcelain", "--", *paths], text=True)
        if ending_commit != commit or ending_status:
            raise RuntimeError("tested source changed during the consumer run")
        report["status"] = "Pass"
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        UTIL.save(out / "summary.json", report)
        raise
    UTIL.save(out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

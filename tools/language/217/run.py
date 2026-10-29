#!/usr/bin/env python3
"""Replay frozen readiness probes and the five-module synchronous consumer.

This Linux runner uses an already built, identified compiler. It never builds
Rust or edits compiler semantics. The separate linker probe wraps actual
realloc/free; the normal executable stays the compiler's unmodified output.
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
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def command(argv, directory, data=b"", environment=None):
    directory.mkdir(parents=True, exist_ok=False)
    argv = [str(x) for x in argv]
    record = {"command": argv, "stdin_hex": data.hex(), "start": time.time(),
              "environment": environment or {}, "status": "Running"}
    save(directory / "command.json", record)
    try:
        result = subprocess.run(argv, input=data, capture_output=True, timeout=60,
                                env=dict(os.environ, **(environment or {})))
        (directory / "stdout").write_bytes(result.stdout)
        (directory / "stderr").write_bytes(result.stderr)
        record.update(status="Completed", exit=result.returncode,
                      stdout_sha256=digest(directory / "stdout"),
                      stderr_sha256=digest(directory / "stderr"))
        return result
    except subprocess.TimeoutExpired as error:
        (directory / "stdout").write_bytes(error.stdout or b"")
        (directory / "stderr").write_bytes(error.stderr or b"")
        record.update(status="Timeout", exit=None)
        raise
    finally:
        record["end"] = time.time()
        save(directory / "command.json", record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-build-receipt", type=Path, required=True)
    parser.add_argument("--compiler-sha256", required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    git = lambda *words: subprocess.check_output(["git", "-C", str(repo), *words], text=True).strip()
    commit, dirty = git("rev-parse", "HEAD"), git("status", "--porcelain")
    if dirty:
        (out / "dirty.txt").write_text(dirty)
        raise RuntimeError("a committed clean consumer checkout is required")
    build_receipt = json.loads(args.compiler_build_receipt.read_text())
    if (build_receipt.get("status") != "Pass" or build_receipt.get("exit_code") != 0
            or build_receipt.get("git_status") != ""):
        raise RuntimeError("compiler build receipt must identify a clean successful build")
    compiler_commit = build_receipt["source_commit"]
    if digest(cli) != args.compiler_sha256 or build_receipt["cli_sha256"] != args.compiler_sha256:
        raise RuntimeError("compiler executable identity mismatch")
    production = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                  ":(glob)crates/*/Cargo.toml", ":(glob)crates/*/build.rs", ":(glob)crates/*/src/**"]
    if git("diff", "--name-only", compiler_commit, "HEAD", "--", *production):
        raise RuntimeError("compiler production inputs differ from the frozen CLI")
    tool = repo / "tools/language/217"
    manifest = json.loads((tool / "SOURCE-MANIFEST.json").read_text())
    for path, expected in manifest["files"].items():
        if digest(repo / path) != expected:
            raise RuntimeError("input differs from the frozen manifest: " + path)
    oracle = json.loads((tool / "ORACLE.json").read_text())
    probes = json.loads((tool / "PROBES.json").read_text())
    report = {"status": "Running", "source_commit": commit,
              "compiler_source_commit": compiler_commit, "cli_sha256": digest(cli),
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt),
              "source_manifest_sha256": digest(tool / "SOURCE-MANIFEST.json"),
              "oracle_sha256": digest(tool / "ORACLE.json"), "sources": manifest["files"],
              "platform": "Linux native", "native_windows": "NotRun", "wine": "NotRun",
              "full_language": "NotSatisfied", "full_zui": "NotClaimed", "product": "NotRun",
              "environment": {k: os.environ.get(k) for k in ["ZIL_CLANG", "LD_LIBRARY_PATH", "TMPDIR"]},
              "probes": [], "builds": [], "observations": [], "indexed_control": []}
    save(out / "summary.json", report)
    (out / "compiler-build-receipt.json").write_bytes(args.compiler_build_receipt.read_bytes())
    (out / "oracle.json").write_bytes((tool / "ORACLE.json").read_bytes())

    def observe(executable, directory, row, expected, opt, instrumented):
        environment = {"ZIL_TEST_FAIL_AT": str(expected["fail_at"])} if instrumented else None
        result = command([executable], directory, row["input"].encode(), environment)
        actual = {"scenario": row["name"], "opt": opt, "instrumented": instrumented,
                  "fail_at": expected.get("fail_at", 0), "exit": result.returncode,
                  "stdout": result.stdout.decode(errors="replace")}
        if result.returncode != expected["exit"] or result.stdout != expected["stdout"].encode():
            raise RuntimeError(f"{directory}: native outcome differs: {actual}, expected {expected}")
        if instrumented:
            allocation = json.loads(result.stderr)
            for key, value in expected["allocation"].items():
                if allocation.get(key) != value:
                    raise RuntimeError(f"{directory}: {key}={allocation.get(key)}, expected {value}")
            actual["allocation"] = allocation
        elif result.stderr:
            raise RuntimeError(f"{directory}: unexpected stderr {result.stderr!r}")
        report["observations"].append(actual)
        save(out / "summary.json", report)

    try:
        for case in probes["cases"]:
            directory = out / "probes" / case["name"]
            result = command([cli, "check", repo / case["path"], "--json"], directory)
            checked = json.loads(result.stdout)
            if result.returncode != case["expected"]["exit"]:
                raise RuntimeError(f"{case['name']}: unexpected process exit {result.returncode}")
            for field in ("status", "phase", "verification_runs"):
                if checked[field] != case["expected"][field]:
                    raise RuntimeError(f"{case['name']}: unexpected {field}: {checked[field]}")
            codes = [d["code"] for d in checked["diagnostics"]]
            if codes != case["expected"]["codes"]:
                raise RuntimeError(f"{case['name']}: unexpected diagnostics {codes}")
            report["probes"].append({"name": case["name"], "status": checked["status"],
                                     "phase": checked["phase"], "verification_runs": checked["verification_runs"],
                                     "source_sha256": digest(repo / case["path"]), "purpose": case["purpose"]})
        source = repo / "examples/mainline/zui_start217/main.zil"
        for opt in ("O0", "O2"):
            directory = out / opt
            executable, phases = directory / "program", directory / "phases"
            result = command([cli, "build", source, "--opt", opt, "--clang", args.clang,
                              "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
            built = json.loads(result.stdout)
            if (result.returncode or built["status"] != "Success"
                    or built["verification_runs"] != 1 or len(built["source_files"]) != 5):
                raise RuntimeError(f"{opt}: five-module build failed: {built}")
            probe = directory / "allocation-probe"
            linked = command([args.clang, "-" + opt, phases / "llvm.ll",
                              repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                              "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt}: allocation shim link failed")
            report["builds"].append({"opt": opt, "verification_runs": 1, "module_count": 5,
                                      "executable_sha256": digest(executable),
                                      "llvm_sha256": digest(phases / "llvm.ll"),
                                      "allocation_probe_sha256": digest(probe)})
            for row in oracle["rows"]:
                observe(executable, directory / row["name"] / "ordinary", row, row["ordinary"], opt, False)
                for expected in row["probes"]:
                    observe(probe, directory / row["name"] / f"fail-{expected['fail_at']}",
                            row, expected, opt, True)
            control = repo / "examples/mainline/zui_start217/indexed_control/main.zil"
            control_bin = directory / "indexed-control"
            result = command([cli, "build", control, "--opt", opt, "--clang", args.clang,
                              "-o", control_bin, "--json"], directory / "control-build")
            built = json.loads(result.stdout)
            if result.returncode or built["status"] != "Success" or built["verification_runs"] != 1:
                raise RuntimeError("indexed representation control no longer builds")
            actual = command([control_bin], directory / "control-run")
            if actual.returncode != 0 or actual.stdout != b"B!AC" or actual.stderr:
                raise RuntimeError("indexed representation control outcome changed")
            report["indexed_control"].append({"opt": opt, "verification_runs": 1,
                                                "exit": actual.returncode, "stdout": actual.stdout.decode()})
        counts = {"ordinary": sum(not r["instrumented"] for r in report["observations"]),
                  "instrumented": sum(r["instrumented"] for r in report["observations"]),
                  "allocation_failures": sum(r.get("allocation", {}).get("failures", 0)
                                             for r in report["observations"])}
        if counts != oracle["planned_executions"] or len(report["probes"]) != len(probes["cases"]):
            raise RuntimeError("actual selection/execution count differs from the frozen plan")
        if git("rev-parse", "HEAD") != commit or git("status", "--porcelain"):
            raise RuntimeError("source checkout changed during execution")
        for path, expected in manifest["files"].items():
            if digest(repo / path) != expected:
                raise RuntimeError("an input changed during execution: " + path)
        if digest(cli) != args.compiler_sha256:
            raise RuntimeError("compiler executable changed during execution")
        report.update(status="Pass", counts=counts, probe_count=len(report["probes"]),
                      inputs_unchanged=True, end_git_status="")
    except BaseException as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ["status", "source_commit", "counts", "probe_count"]}, indent=2))


if __name__ == "__main__":
    main()

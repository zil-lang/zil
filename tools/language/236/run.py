#!/usr/bin/env python3
"""Replay frozen 236 source with an identified CLI; never builds Rust.

Ordinary execution uses the original CLI artifact. Fault injection relinks the
actual captured LLVM API object with the existing real realloc/free wrapper.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def command(argv, directory, data=b"", environment=None):
    directory.mkdir(parents=True, exist_ok=False)
    argv = [str(word) for word in argv]
    record = {"command": argv, "stdin_hex": data.hex(), "environment": environment or {},
              "start": time.time(), "status": "Running"}
    save(directory / "command.json", record)
    try:
        result = subprocess.run(argv, input=data, capture_output=True, timeout=60,
                                env=dict(os.environ, **(environment or {})))
        (directory / "stdout").write_bytes(result.stdout)
        (directory / "stderr").write_bytes(result.stderr)
        record.update(status="Completed", exit=result.returncode)
        return result
    except subprocess.TimeoutExpired as error:
        record.update(status="Timeout", exit=None)
        (directory / "stdout").write_bytes(error.stdout or b"")
        (directory / "stderr").write_bytes(error.stderr or b"")
        raise
    finally:
        record["end"] = time.time()
        save(directory / "command.json", record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-build-receipt", type=Path, required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    git = lambda *words: subprocess.check_output(["git", "-C", str(repo), *words], text=True).strip()
    commit, dirty = git("rev-parse", "HEAD"), git("status", "--porcelain")
    if dirty:
        raise RuntimeError("consumer source must be committed and clean")
    receipt = json.loads(args.compiler_build_receipt.read_text())
    if (receipt.get("status") != "Pass" or receipt.get("exit_code") != 0
            or receipt.get("git_status") != "" or not receipt.get("source_commit")
            or receipt.get("cli_sha256") != digest(cli)):
        raise RuntimeError("receipt must bind the actual CLI to a clean successful compiler build")
    tool = repo / "tools/language/236"
    manifest = json.loads((tool / "SOURCE-MANIFEST.json").read_text())
    for relative, expected in manifest["files"].items():
        if digest(repo / relative) != expected:
            raise RuntimeError("frozen input changed: " + relative)
    oracle = json.loads((tool / "ORACLE.json").read_text())
    out.mkdir(parents=True, exist_ok=False)
    (out / "compiler-build-receipt.json").write_bytes(args.compiler_build_receipt.read_bytes())
    report = {"status": "Running", "consumer_commit": commit,
              "compiler_commit": receipt["source_commit"], "cli_sha256": digest(cli),
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt),
              "source_manifest_sha256": digest(tool / "SOURCE-MANIFEST.json"),
              "oracle_sha256": digest(tool / "ORACLE.json"),
              "full_language": "NotSatisfied", "full_zui": "NotClaimed", "product": "NotRun",
              "native_windows": "NotRun", "wine": "NotRun", "builds": [], "observations": []}
    save(out / "RESULT.json", report)

    def observe(executable, directory, row, expected, opt, instrumented):
        environment = {"ZIL_TEST_FAIL_AT": str(expected["fail_at"])} if instrumented else None
        actual = command([executable], directory, row["input"].encode(), environment)
        observed = {"scenario": row["name"], "opt": opt, "instrumented": instrumented,
                    "fail_at": expected["fail_at"], "exit": actual.returncode,
                    "stdout": actual.stdout.decode(errors="replace")}
        if actual.returncode != expected["exit"] or actual.stdout != expected["stdout"].encode():
            raise RuntimeError(f"{directory}: {observed}, expected {expected}")
        if instrumented:
            allocation = json.loads(actual.stderr)
            observed["allocation"] = allocation
            for key, value in expected["allocation"].items():
                if allocation.get(key) != value:
                    raise RuntimeError(f"{directory}: {key}={allocation.get(key)}, expected {value}")
        elif actual.stderr:
            raise RuntimeError(f"{directory}: unexpected stderr {actual.stderr!r}")
        report["observations"].append(observed)
        save(out / "RESULT.json", report)

    try:
        for opt in ("O0", "O2"):
            directory = out / opt
            directory.mkdir()
            wrapper, captured = directory / "capture-object.sh", directory / "api.o"
            wrapper.write_text("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\n"
                               "case \"$arg\" in *.o|*.obj) cp -- \"$arg\" " + shlex.quote(str(captured))
                               + " ;; esac\ndone\nexec " + shlex.quote(args.clang) + " \"$@\"\n")
            wrapper.chmod(0o700)
            executable, phases = directory / "program", directory / "phases"
            built = command([cli, "build", repo / "examples/mainline/zui_core236/main.zil",
                             "--opt", opt, "--backend", "llvm-api", "--clang", wrapper,
                             "-o", executable, "--emit-dir", phases, "--json"], directory / "build")
            build = json.loads(built.stdout)
            if (built.returncode or build["status"] != "Success"
                    or build["verification_runs"] != 1 or len(build["source_files"]) != 5
                    or not captured.is_file()):
                raise RuntimeError(f"{opt}: five-module native build did not succeed: {build}")
            native = build["native_codegen"]
            if native["backend"] != "llvm-api" or native.get("fallback") is not None:
                raise RuntimeError("LLVM API object identity not established")
            # The captured object, rather than regenerated text LLVM, is linked.
            probe = directory / "probe"
            linked = command([args.clang, captured, repo / "tools/language/173/allocator_probe.c",
                              "-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
            if linked.returncode:
                raise RuntimeError("actual-object allocator probe link failed")
            report["builds"].append({"opt": opt, "verification_runs": 1,
                                      "executable_sha256": digest(executable),
                                      "api_object_sha256": digest(captured), "probe_sha256": digest(probe)})
            for row in oracle["rows"]:
                observe(executable, directory / row["name"] / "ordinary", row, row["ordinary"], opt, False)
                for expected in row["probes"]:
                    observe(probe, directory / row["name"] / f"fail-{expected['fail_at']}",
                            row, expected, opt, True)
        ordinary = sum(not row["instrumented"] for row in report["observations"])
        probes = sum(row["instrumented"] for row in report["observations"])
        if ordinary != 2 * oracle["ordinary_per_optimization"] or probes != 2 * oracle["probes_per_optimization"]:
            raise RuntimeError("execution selection differs from frozen plan")
        if git("rev-parse", "HEAD") != commit or git("status", "--porcelain") or digest(cli) != report["cli_sha256"]:
            raise RuntimeError("source/compiler identity changed during execution")
        for relative, expected in manifest["files"].items():
            if digest(repo / relative) != expected:
                raise RuntimeError("frozen input changed during execution: " + relative)
        report.update(status="Pass", ordinary_executions=ordinary, allocator_executions=probes)
    except Exception as error:
        report.update(status="Fail", error=str(error))
        raise
    finally:
        save(out / "RESULT.json", report)
        artifacts = {str(path.relative_to(out)): digest(path) for path in sorted(out.rglob("*")) if path.is_file()}
        save(out / "ARTIFACT-MANIFEST.json", {"sha256": artifacts})


if __name__ == "__main__":
    main()

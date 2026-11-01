#!/usr/bin/env python3
"""Observe baseline Unsupported and calibrate real leaf/API-object allocation ABI.

Pass here refers only to the existing-shape control. It cannot promote the
combined 227/235 consumer. No compiler build or full suite is run.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
sys.dont_write_bytecode = True
from run import command, digest, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-source-commit", required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True).strip()
    if dirty:
        raise RuntimeError("freeze source before controls")
    manifest = json.loads((repo / "tools/language/236/SOURCE-MANIFEST.json").read_text())
    for relative, expected in manifest["files"].items():
        if digest(repo / relative) != expected:
            raise RuntimeError("frozen input changed: " + relative)
    out.mkdir(parents=True, exist_ok=False)
    report = {"status": "Running", "scope": "baseline rejection and existing leaf only",
              "consumer_commit": commit, "compiler_source_commit_supplied_by_integrator": args.compiler_source_commit,
              "cli_sha256": digest(cli), "combined_consumer": "NotRun", "full_language": "NotSatisfied",
              "full_zui": "NotClaimed", "checks": [], "leaf": []}
    source = repo / "examples/mainline/zui_core236"
    try:
        for name in ("main", "defer_control"):
            result = command([cli, "check", source / f"{name}.zil", "--json"], out / f"check-{name}")
            checked = json.loads(result.stdout)
            if (result.returncode != 3 or checked["status"] != "Unsupported" or checked["phase"] != "types"
                    or checked["verification_runs"] != 0
                    or [item["code"] for item in checked["diagnostics"]] != ["E-UNSUPPORTED"]):
                raise RuntimeError(f"baseline {name} outcome changed: {checked}")
            report["checks"].append({"name": name, "status": checked["status"], "phase": checked["phase"],
                                      "codes": [item["code"] for item in checked["diagnostics"]]})
        for opt in ("O0", "O2"):
            directory = out / opt
            directory.mkdir()
            captured, wrapper = directory / "api.o", directory / "capture-object.sh"
            wrapper.write_text("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\n"
                               "case \"$arg\" in *.o|*.obj) cp -- \"$arg\" " + shlex.quote(str(captured))
                               + " ;; esac\ndone\nexec " + shlex.quote(args.clang) + " \"$@\"\n")
            wrapper.chmod(0o700)
            binary = directory / "program"
            result = command([cli, "build", source / "leaf_control.zil", "--backend", "llvm-api",
                              "--opt", opt, "--clang", wrapper, "-o", binary,
                              "--emit-dir", directory / "phases", "--json"], directory / "build")
            built = json.loads(result.stdout)
            if result.returncode or built["status"] != "Success" or built["verification_runs"] != 1 or not captured.is_file():
                raise RuntimeError(f"leaf build failed: {built}")
            native = built["native_codegen"]
            if native["backend"] != "llvm-api" or native["fallback"] is not None or native["actual_units"] != 1:
                raise RuntimeError("leaf backend identity differs")
            ordinary = command([binary], directory / "ordinary")
            if ordinary.returncode or ordinary.stdout != b"A" or ordinary.stderr:
                raise RuntimeError("leaf ordinary output differs")
            probe = directory / "probe"
            result = command([args.clang, captured, repo / "tools/language/173/allocator_probe.c",
                              "-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o", probe], directory / "link")
            if result.returncode:
                raise RuntimeError("leaf real-object probe link failed")
            for fail in (0, 1, 2):
                actual = command([probe], directory / f"fail-{fail}", environment={"ZIL_TEST_FAIL_AT": str(fail)})
                failed = fail == 1
                expected = {"alloc_calls": 1, "allocated": int(not failed), "frees": int(not failed),
                            "failures": int(failed), "live": 0, "peak_live": int(not failed)}
                stats = json.loads(actual.stderr)
                if actual.returncode != int(failed) or actual.stdout != (b"FA" if failed else b"A"):
                    raise RuntimeError("leaf injected output differs")
                if any(stats.get(key) != value for key, value in expected.items()):
                    raise RuntimeError(f"leaf allocation ABI differs: {stats}, expected {expected}")
                report["leaf"].append({"opt": opt, "fail_at": fail, "exit": actual.returncode,
                                         "stdout": actual.stdout.decode(), "allocation": stats,
                                         "api_object_sha256": digest(captured)})
        report["status"] = "Pass"
    except Exception as error:
        report.update(status="Fail", error=str(error))
        raise
    finally:
        save(out / "RESULT.json", report)
        save(out / "ARTIFACT-MANIFEST.json", {"sha256": {
            str(path.relative_to(out)): digest(path) for path in sorted(out.rglob("*")) if path.is_file()}})


if __name__ == "__main__":
    main()

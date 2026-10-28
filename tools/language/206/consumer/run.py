#!/usr/bin/env python3
"""Run the frozen five-module complete-Product consumer from actual ZIL source.

The independently prepared oracle is immutable data, not recomputed from the
compiler or native results. Original draft metadata stays PredeclaredModelOnly;
this runner records separate actual compiler, platform and execution identities.
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
               "stdin_hex": input_bytes.hex(), "controlled_environment": environment or {},
               "inherited_environment": {key: os.environ.get(key) for key in
                                         ["PATH", "ZIL_CLANG", "LD_LIBRARY_PATH", "WINEPREFIX",
                                          "WINESERVER", "WINELOADER", "WINEDEBUG", "TMPDIR"]}}
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



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--compiler-build-receipt", type=Path, required=True)
    parser.add_argument("--compiler-sha256", required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--platform", choices=["linux", "win64"], required=True)
    parser.add_argument("--wine")
    parser.add_argument("--wine-library-path")
    parser.add_argument("--linker")
    parser.add_argument("--windows-lib", action="append", default=[])
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
    if dirty:
        (out / "dirty.txt").write_text(dirty)
        raise RuntimeError("consumer acceptance requires a committed source checkout")
    compiler_receipt = json.loads(args.compiler_build_receipt.read_text())
    if (compiler_receipt.get("status") != "Pass" or compiler_receipt.get("exit_code") != 0
            or compiler_receipt.get("git_status") != ""):
        raise RuntimeError("compiler identity requires a successful clean build receipt")
    compiler_commit = compiler_receipt["source_commit"]
    if digest(cli) != args.compiler_sha256:
        raise RuntimeError("compiler executable differs from the frozen build digest")
    # Consumer files/tests/docs may follow the compiler build, but production
    # Rust, manifests and build scripts must still be the actual same inputs.
    production_paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
                        ":(glob)crates/*/Cargo.toml", ":(glob)crates/*/build.rs",
                        ":(glob)crates/*/src/**"]
    different = subprocess.check_output(
        ["git", "-C", str(repo), "diff", "--name-only", compiler_commit, "HEAD", "--", *production_paths],
        text=True)
    if different:
        (out / "compiler-source-difference.txt").write_text(different)
        raise RuntimeError("compiler production inputs differ from this source checkout")
    oracle_path = repo / "tools/language/206/consumer/predeclared/ORACLE.json"
    if digest(oracle_path) != "956e2142fc79cd4ea0431c5cfecdaa8d54d47072873795e88a3c8d1df929ec6d":
        raise RuntimeError("the frozen independent oracle changed")
    declaration = json.loads(oracle_path.read_text())
    oracles = declaration["oracles"]
    expected_counts = declaration["planned_executions_per_platform"]
    original_sources = json.loads((oracle_path.parent / "SOURCE-MANIFEST.json").read_text())["files"]
    normative_inputs = json.loads((repo / "data/normative-inputs.json").read_text())
    for name, expected_hash in normative_inputs["files"].items():
        if digest(repo / name) != expected_hash:
            raise RuntimeError("normative input differs from the actual manifest: " + name)
    sources = repo / "examples/mainline/package_product206"
    frozen_sources = out / "sources"
    frozen_sources.mkdir()
    source_hashes = {}
    for path in sorted(sources.glob("*.zil")):
        data = path.read_bytes()
        (frozen_sources / path.name).write_bytes(data)
        source_hashes[path.name] = hashlib.sha256(data).hexdigest()
        if (source_hashes[path.name] != original_sources[path.name]["sha256"]
                or len(data) != original_sources[path.name]["bytes"]):
            raise RuntimeError("source differs from the fixed predeclared input: " + path.name)
    if set(source_hashes) != set(original_sources):
        raise RuntimeError("the five-module source set differs from the fixed input")
    actual_tools = out / "actual-tools"
    actual_tools.mkdir()
    tool_hashes = {}
    for name in ["tools/language/206/consumer/run.py", "tools/language/173/allocator_probe.c",
                 "tools/research/142/allocator_probe.c", "tools/language/181/heap_probe.c"]:
        path = repo / name
        data = path.read_bytes()
        target = actual_tools / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        tool_hashes[name] = hashlib.sha256(data).hexdigest()
    (out / "predeclared-oracle.json").write_bytes(oracle_path.read_bytes())
    (out / "compiler-build-receipt.json").write_bytes(args.compiler_build_receipt.read_bytes())
    report = {"status": "Running", "source_commit": commit, "cli_sha256": digest(cli),
              "compiler_source_commit": compiler_commit,
              "compiler_build_receipt": str(args.compiler_build_receipt.resolve()),
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt),
              "compiler_production_matches_checkout": True,
              "design_sha256": digest(repo / "_zil_design.md"),
              "normative_inputs": normative_inputs,
              "sources": source_hashes,
              "allocator_sources": {str(p.relative_to(repo)): digest(p) for p in
                                    [repo / "tools/language/173/allocator_probe.c",
                                     repo / "tools/language/181/heap_probe.c"]},
              "oracles": oracles, "builds": [], "observations": [],
              "oracle_sha256": digest(oracle_path), "oracle_status": declaration["status"],
              "actual_tools": tool_hashes, "predeclared_source_bytes_equal": True,
              "planned_executions_per_platform": expected_counts,
              "platform": args.platform, "execution": "Wine compatibility" if args.wine else "native", "native_windows": "NotRun",
              "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", report)

    def execute(program, directory, input_bytes, environment=None):
        argv = [program]
        environment = dict(environment or {})
        if args.wine:
            argv.insert(0, args.wine)
            environment["LD_LIBRARY_PATH"] = args.wine_library_path
        return command(argv, directory, input_bytes, environment)

    def observe(result, row, opt, mode, expected):
        if result.returncode != expected["exit"] or result.stdout != expected["stdout"].encode():
            raise RuntimeError(f"{row['name']}/{opt}/{mode}: "
                               f"exit={result.returncode}, stdout={result.stdout!r}, stderr={result.stderr!r}")
        observation = {"scenario": row["name"], "optimization": opt, "mode": mode,
                       "stdin_hex": row["input"].encode().hex(), "stdout": result.stdout.decode(),
                       "exit_code": result.returncode, "fail_at": expected.get("fail_at", 0)}
        if "allocation" in expected:
            actual = json.loads(result.stderr)
            allocation_expected = dict(expected["allocation"])
            if args.platform == "win64":
                allocation_expected["bad_operations"] = 0
            if actual != allocation_expected:
                raise RuntimeError(f"{row['name']}/{opt}/{mode}: allocation {actual}, expected {expected['allocation']}")
            observation["allocation"] = actual
        elif result.stderr:
            raise RuntimeError(f"unexpected ordinary stderr: {result.stderr!r}")
        report["observations"].append(observation)
        save(out / "summary.json", report)

    try:
        for opt in ("O0", "O2"):
            directory = out / opt
            suffix = ".exe" if args.platform == "win64" else ""
            executable, probe, phases = directory / ("program" + suffix), directory / ("allocation-probe" + suffix), directory / "phases"
            argv = [cli, "build", sources / "main.zil", "--opt", opt, "--clang", args.clang,
                    "-o", executable, "--emit-dir", phases, "--json"]
            if args.platform == "win64":
                assert args.wine and args.wine_library_path and args.linker and len(args.windows_lib) == 3
                argv += ["--target", "win64", "--linker", args.linker]
                for library in args.windows_lib:
                    argv += ["--windows-lib", library]
            build = command(argv, directory / "build")
            compiled = json.loads(build.stdout)
            if (build.returncode or compiled["status"] != "Success"
                    or compiled["verification_runs"] != 1 or len(compiled["source_files"]) != 5):
                raise RuntimeError(f"{opt}: {compiled}")
            report["builds"].append({"optimization": opt, "verification_runs": 1,
                                     "llvm_sha256": digest(phases / "llvm.ll"),
                                     "executable_sha256": digest(executable),
                                     "source_files": compiled["source_files"], "module_count": 5})
            save(out / "summary.json", report)
            if args.platform == "win64":
                original = phases / "llvm.ll"
                text = original.read_text()
                counts = {}
                for symbol in ["HeapAlloc", "HeapReAlloc", "HeapFree", "ExitProcess"]:
                    pattern = "@" + symbol + "("
                    counts[symbol] = text.count(pattern)
                    assert counts[symbol], symbol
                    text = text.replace(pattern, "@zil_probe_" + symbol + "(")
                altered = directory / "probe.ll"
                altered.write_text(text)
                ir_object, shim_object = directory / "probe.obj", directory / "heap_probe.obj"
                for label, command_argv in [
                    ("probe-ir", [args.clang, "--target=x86_64-pc-windows-msvc", "-x", "ir", "-" + opt,
                                  "-c", altered, "-o", ir_object]),
                    ("probe-shim", [args.clang, "--target=x86_64-pc-windows-msvc", "-" + opt,
                                    "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
                                    "-Wall", "-Wextra", "-Werror", "-c", repo / "tools/language/181/heap_probe.c",
                                    "-o", shim_object]),
                    ("probe-link", [args.linker, "/machine:x64", "/subsystem:windows", "/entry:zil.pe.entry",
                                   "/nodefaultlib", "/timestamp:0", "/out:" + str(probe),
                                   ir_object, shim_object, *args.windows_lib]),
                ]:
                    linked = command(command_argv, directory / label)
                    if linked.returncode:
                        raise RuntimeError(f"{opt}/{label} failed: {linked.stderr!r}")
                report["builds"][-1].update(probe_llvm_sha256=digest(altered),
                                            redirected_symbols=counts, instrumentation="separate exact external-symbol redirection")
            else:
                linked = command([args.clang, "-" + opt, phases / "llvm.ll",
                                          repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                                          "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                if linked.returncode:
                    raise RuntimeError(f"{opt} allocation shim failed to link")
            report["builds"][-1]["allocation_probe_sha256"] = digest(probe)
            save(out / "summary.json", report)
            for row in report["oracles"]:
                case = directory / row["name"]
                ordinary = execute(executable, case / "ordinary", row["input"].encode())
                observe(ordinary, row, opt, "ordinary", row)
                for expected in row["probes"]:
                    mode = expected["name"]
                    result = execute(probe, case / mode, row["input"].encode(),
                                     {"ZIL_TEST_FAIL_AT": str(expected["fail_at"])})
                    observe(result, row, opt, mode, expected)
        actual_counts = {"ordinary": sum(r["mode"] == "ordinary" for r in report["observations"]),
                         "instrumented": sum(r["mode"] != "ordinary" for r in report["observations"]),
                         "total": len(report["observations"]),
                         "actual_allocation_failures": sum(r.get("allocation", {}).get("failures", 0)
                                                           for r in report["observations"])}
        if actual_counts != expected_counts:
            raise RuntimeError(f"actual execution counts differ: {actual_counts}, expected {expected_counts}")
        end_commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
        end_status = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
        if end_commit != commit or end_status:
            raise RuntimeError("the source checkout changed during the consumer run")
        for name, expected_hash in source_hashes.items():
            if digest(sources / name) != expected_hash:
                raise RuntimeError("a consumer input changed during execution")
        for name, expected_hash in tool_hashes.items():
            if digest(repo / name) != expected_hash:
                raise RuntimeError("an actual tool input changed during execution")
        if digest(cli) != args.compiler_sha256:
            raise RuntimeError("the compiler executable changed during execution")
        report.update(end_source_commit=end_commit, end_git_status=end_status,
                      inputs_unchanged_at_end=True)
        report.update(status="Pass", executions=len(report["observations"]),
                      native_executions=len(report["observations"]) if args.platform == "linux" else 0,
                      wine_executions=len(report["observations"]) if args.platform == "win64" else 0,
                      ordinary_executions=sum(r["mode"] == "ordinary" for r in report["observations"]),
                      instrumented_executions=sum(r["mode"] != "ordinary" for r in report["observations"]),
                      actual_allocation_failures=sum(r.get("allocation", {}).get("failures", 0)
                                                     for r in report["observations"]))
    except BaseException as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ("status", "source_commit", "executions", "native_executions", "wine_executions",
                                             "ordinary_executions", "instrumented_executions",
                                             "actual_allocation_failures")}, indent=2))


if __name__ == "__main__":
    main()

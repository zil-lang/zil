#!/usr/bin/env python3
"""Exercise generic Store success unions using predeclared native oracles."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess


def scenarios():
    # Each tuple is fail_at, expected stdout, completed allocations, exit.
    # Initial reserve fails before either input was moved: reverse parameter
    # cleanup. Growth fails after one insertion: live Store prefix first.
    common = [(1, b"BAM", 0, 1), (2, b"ABM", 1, 1),
              (3, b"DCABM", 1, 1), (4, b"CDABM", 2, 1)]
    rows = [
        ("success", b"0", b"ZCDABM", 0, 2, common),
        ("first-factory-error", b"1", b"ABM", 1, 1, common[:2]),
        ("second-factory-error", b"2", b"CDABM", 1, 2, common),
        ("caller-error", b"3", b"CDABM", 1, 2, common),
        ("discard", b"4", b"ABM", 0, 1,
         [(1, b"BAM", 0, 0), (2, b"ABM", 1, 0)]),
    ]
    return [dict(name=name, input=data, stdout=output, exit=exit_code,
                 allocated=allocated, failures=failures)
            for name, data, output, exit_code, allocated, failures in rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--platform", choices=["linux", "win64"], required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--linker")
    parser.add_argument("--windows-lib", action="append", default=[])
    parser.add_argument("--wine")
    parser.add_argument("--wine-library-path")
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location("commands177", repo / "tools/language/177/consumer.py")
    shared = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(shared)
    command, digest, save = shared.command, shared.digest, shared.save
    sources = repo / "examples/mainline/generic_result186"
    report = {"status": "Running", "source_commit": subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
        "dirty": subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True),
        "cli_sha256": digest(cli), "platform": args.platform,
        "execution": "Wine compatibility" if args.wine else "native",
        "source_hashes": {}, "builds": [], "observations": [],
        "full_language": "NotSatisfied", "product": "NotRun"}
    inputs = [*sorted(sources.glob("*.zil")), Path(__file__).resolve(),
              repo / "tools/language/177/consumer.py", repo / "tools/language/173/allocator_probe.c",
              repo / "tools/research/142/allocator_probe.c", repo / "tools/language/181/heap_probe.c"]
    for path in inputs:
        report["source_hashes"][str(path.relative_to(repo))] = digest(path)
    save(out / "summary.json", report)

    def run(executable, directory, row, fail_at=0):
        argv = [str(executable)]
        environment = {"ZIL_TEST_FAIL_AT": str(fail_at)}
        if args.wine:
            argv.insert(0, args.wine)
            if args.wine_library_path:
                environment["LD_LIBRARY_PATH"] = args.wine_library_path
        return command(argv, directory, row["input"], environment)

    def observe(result, row, opt, mode, expected, exit_code, allocated=None, fail_at=0):
        if result.returncode != exit_code or result.stdout != expected:
            raise RuntimeError(f"{opt}/{row['name']}/{mode}: exit={result.returncode}, "
                               f"stdout={result.stdout!r}, stderr={result.stderr!r}")
        observation = {"scenario": row["name"], "optimization": opt, "mode": mode,
                       "stdin_hex": row["input"].hex(), "stdout": result.stdout.decode(),
                       "exit_code": result.returncode, "fail_at": fail_at}
        if allocated is not None:
            allocation = json.loads(result.stderr)
            if (allocation["allocated"] != allocated or allocation["frees"] != allocated
                    or allocation["failures"] != int(fail_at > 0) or allocation["live"]
                    or allocation.get("bad_operations", 0)):
                raise RuntimeError(f"{opt}/{row['name']}/{mode}: heap imbalance {allocation}")
            observation["allocation"] = allocation
        elif result.stderr:
            raise RuntimeError(f"ordinary stderr is not empty: {result.stderr!r}")
        report["observations"].append(observation)
        save(out / "summary.json", report)

    try:
        for opt in ["O0", "O2"]:
            directory = out / opt
            phases = directory / "phases"
            executable = directory / ("program.exe" if args.platform == "win64" else "program")
            probe = directory / ("probe.exe" if args.platform == "win64" else "probe")
            build_args = [cli, "build", sources / "main.zil", "--opt", opt, "--clang", args.clang,
                          "-o", executable, "--emit-dir", phases, "--json"]
            if args.platform == "win64":
                if not args.linker or not args.windows_lib:
                    raise ValueError("Win64 needs explicit linker and import libraries")
                build_args += ["--target", "win64", "--linker", args.linker]
                for library in args.windows_lib:
                    build_args += ["--windows-lib", library]
            result = command(build_args, directory / "build")
            compiled = json.loads(result.stdout)
            if result.returncode or compiled["status"] != "Success" or compiled["verification_runs"] != 1:
                raise RuntimeError(f"{opt} compiler rejection: {compiled}")
            if len(compiled["source_files"]) != 4:
                raise RuntimeError("consumer must traverse its four actual modules")
            llvm = phases / "llvm.ll"
            build = {"optimization": opt, "verification_runs": 1,
                     "source_files": compiled["source_files"], "module_count": 4,
                     "llvm_sha256": digest(llvm), "executable_sha256": digest(executable)}
            report["builds"].append(build)
            if args.platform == "win64":
                text = llvm.read_text()
                replacements = {}
                for symbol in ["HeapAlloc", "HeapReAlloc", "HeapFree", "ExitProcess"]:
                    pattern = "@" + symbol + "("
                    replacements[symbol] = text.count(pattern)
                    if not replacements[symbol]:
                        raise RuntimeError("missing checked heap ABI symbol: " + symbol)
                    text = text.replace(pattern, "@zil_probe_" + symbol + "(")
                probe_llvm = directory / "probe.ll"
                probe_llvm.write_text(text)
                build.update(instrumentation="exact external symbol redirection",
                             symbol_counts=replacements, probe_llvm_sha256=digest(probe_llvm))
                llvm_obj, shim_obj = directory / "probe.obj", directory / "heap_probe.obj"
                commands = [
                    ("probe-ir", [args.clang, "--target=x86_64-pc-windows-msvc", "-x", "ir", "-"+opt,
                                  "-c", probe_llvm, "-o", llvm_obj]),
                    ("probe-shim", [args.clang, "--target=x86_64-pc-windows-msvc", "-"+opt,
                                    "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
                                    "-Wall", "-Wextra", "-Werror", "-c", repo / "tools/language/181/heap_probe.c",
                                    "-o", shim_obj]),
                    ("probe-link", [args.linker, "/machine:x64", "/subsystem:windows", "/entry:zil.pe.entry",
                                    "/nodefaultlib", "/timestamp:0", "/out:"+str(probe),
                                    llvm_obj, shim_obj, *args.windows_lib]),
                ]
            else:
                commands = [("probe-link", [args.clang, "-"+opt, llvm,
                    repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                    "-Wl,--wrap=free", "-o", probe])]
            for label, argv in commands:
                linked = command(argv, directory / label)
                if linked.returncode:
                    raise RuntimeError(f"{opt}/{label}: {linked.stderr!r}")
            build["allocation_probe_sha256"] = digest(probe)
            save(out / "summary.json", report)
            for row in scenarios():
                case = directory / row["name"]
                result = run(executable, case / "ordinary", row)
                observe(result, row, opt, "ordinary", row["stdout"], row["exit"])
                result = run(probe, case / "allocation", row)
                observe(result, row, opt, "allocation", row["stdout"], row["exit"], row["allocated"])
                for fail_at, expected, allocated, exit_code in row["failures"]:
                    result = run(probe, case / f"failure-{fail_at}", row, fail_at)
                    observe(result, row, opt, "allocation_failure", expected, exit_code, allocated, fail_at)
            invalid = {"name": "invalid-input", "input": b"9"}
            result = run(executable, directory / "invalid-input", invalid)
            observe(result, invalid, opt, "ordinary", b"", 9)
        report.update(status="Pass", executions=len(report["observations"]),
                      ordinary_executions=sum(r["mode"]=="ordinary" for r in report["observations"]),
                      instrumented_executions=sum(r["mode"]!="ordinary" for r in report["observations"]),
                      failure_executions=sum(r["mode"]=="allocation_failure" for r in report["observations"]))
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k:v for k,v in report.items() if k not in ["source_hashes","builds","observations"]},indent=2))


if __name__ == "__main__":
    main()

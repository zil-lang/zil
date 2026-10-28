#!/usr/bin/env python3
"""Run real Linux/Win64 Store consumers and separately instrumented heap probes.

Win64 execution under Wine is labelled compatibility execution, not native
Windows certification. Every ordinary binary is the unchanged CLI output.
The probe redirects four external LLVM symbols and records both input hashes.
Expected output/Drop counts are precomputed independently of compiler output.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--platform", choices=["linux", "win64"], required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--linker")
    parser.add_argument("--windows-lib", action="append", default=[])
    parser.add_argument("--wine")
    parser.add_argument("--wine-library-path", help="isolate Wine shared libraries from compiler libraries")
    parser.add_argument("--case", action="append", choices=["main", "field_exit", "padded", "composed"])
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location("consumer177", repo / "tools/language/177/consumer.py")
    old = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(old)
    command, digest, save = old.command, old.digest, old.save
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
    selected = args.case or ["main", "field_exit", "padded"]
    report = {"status": "Running", "source_commit": commit, "dirty": dirty,
              "cli_sha256": digest(cli), "platform": args.platform,
              "execution": "Wine compatibility" if args.wine else "native",
              "selected": selected, "builds": [], "observations": [],
              "source_hashes": {}, "full_language": "NotSatisfied", "product": "NotRun"}
    for directory in [repo / "examples/mainline/zui_language177", repo / "examples/mainline/zui_language181"]:
        for path in sorted(directory.glob("*.zil")):
            report["source_hashes"][str(path.relative_to(repo))] = digest(path)
    for path in [Path(__file__).resolve(), repo / "tools/language/181/heap_probe.c",
                 repo / "tools/language/177/consumer.py", repo / "tools/language/173/allocator_probe.c",
                 repo / "tools/research/142/allocator_probe.c"]:
        report["source_hashes"][str(path.relative_to(repo))] = digest(path)
    save(out / "summary.json", report)

    def scenarios(name):
        if name == "composed":
            rows = []
            for count in (0, 1, 2, 3):
                output = bytes(range(65, 65 + count)) + b"HZ"
                failures = [(1, b"H", 0, 1), (2, b"H", 1, 1)]
                if count:
                    failures.append((3, b"HZ", 2, 1))
                failures.append((3 + int(count > 0), output, 2 + int(count > 0), 0))
                rows.append({"name": f"count-{count}", "input": str(count).encode(),
                             "stdout": output, "exit": 0, "allocated": 3, "failures": failures})
            rows.append({"name": "invalid-input", "input": b"9", "stdout": b"", "exit": 9,
                         "allocated": 0, "failures": [], "ordinary_only": True})
            return rows
        if name != "padded":
            return old.scenarios(name)
        return [{"name": "padded-item", "input": b"", "stdout": b"CBA", "exit": 0,
                 "allocated": 1, "failures": [(1, b"", 0, 1), (2, b"CBA", 1, 0)]}]

    def run(executable, directory, row, fail_at=0):
        argv = [str(executable)]
        environment = {"ZIL_TEST_FAIL_AT": str(fail_at)}
        if args.wine:
            argv.insert(0, args.wine)
            if args.wine_library_path:
                environment["LD_LIBRARY_PATH"] = args.wine_library_path
        return command(argv, directory, row["input"], environment)

    def observe(result, name, row, opt, mode, expected, exit_code, allocated=None, fail_at=0):
        if result.returncode != exit_code or result.stdout != expected:
            raise RuntimeError(f"{name}/{opt}/{row['name']}/{mode}: exit={result.returncode}, "
                               f"stdout={result.stdout!r}, stderr={result.stderr!r}")
        record = {"entry": name, "scenario": row["name"], "optimization": opt, "mode": mode,
                  "stdin_hex": row["input"].hex(), "stdout": result.stdout.decode(),
                  "exit_code": result.returncode, "fail_at": fail_at}
        if allocated is not None:
            allocation = json.loads(result.stderr)
            if (allocation["live"] or allocation["allocated"] != allocated
                    or allocation["frees"] != allocated
                    or allocation["failures"] != int(fail_at > 0)
                    or allocation.get("bad_operations", 0)):
                raise RuntimeError(f"{name}/{opt}/{row['name']}/{mode}: heap imbalance {allocation}")
            record["allocation"] = allocation
        elif result.stderr:
            raise RuntimeError(f"ordinary program wrote unexpected stderr {result.stderr!r}")
        report["observations"].append(record)
        save(out / "summary.json", report)

    try:
        for name in selected:
            if name in ["padded", "composed"]:
                filename = "platform_padded.zil" if name == "padded" else "composed.zil"
                source = repo / "examples/mainline/zui_language181" / filename
            else:
                source = repo / "examples/mainline/zui_language177" / (name + ".zil")
            for opt in ("O0", "O2"):
                directory = out / name / opt
                executable = directory / ("program.exe" if args.platform == "win64" else "program")
                probe = directory / ("probe.exe" if args.platform == "win64" else "probe")
                phases = directory / "phases"
                build_args = [cli, "build", source, "--opt", opt, "--clang", args.clang,
                              "-o", executable, "--emit-dir", phases, "--json"]
                if args.platform == "win64":
                    if not args.linker or not args.windows_lib:
                        raise ValueError("Win64 requires explicit linker and import libraries")
                    build_args += ["--target", "win64", "--linker", args.linker]
                    for library in args.windows_lib:
                        build_args += ["--windows-lib", library]
                build = command(build_args, directory / "build")
                compiled = json.loads(build.stdout)
                if build.returncode or compiled["status"] != "Success" or compiled["verification_runs"] != 1:
                    raise RuntimeError(f"{name}/{opt} compiler rejection: {compiled}")
                llvm = phases / "llvm.ll"
                row = {"entry": name, "optimization": opt, "verification_runs": 1,
                       "llvm_sha256": digest(llvm), "executable_sha256": digest(executable),
                       "module_count": len(compiled["source_files"]), "source_files": compiled["source_files"]}
                report["builds"].append(row)
                if args.platform == "win64":
                    text = llvm.read_text()
                    replacements = {}
                    for symbol in ["HeapAlloc", "HeapReAlloc", "HeapFree", "ExitProcess"]:
                        pattern = "@" + symbol + "("
                        replacements[symbol] = text.count(pattern)
                        if not replacements[symbol]:
                            raise RuntimeError("missing real Windows ABI symbol " + symbol)
                        text = text.replace(pattern, "@zil_probe_" + symbol + "(")
                    probe_llvm = directory / "probe.ll"
                    probe_llvm.write_text(text)
                    row.update(instrumentation="exact external symbol redirection", symbol_counts=replacements,
                               probe_llvm_sha256=digest(probe_llvm))
                    llvm_obj, shim_obj = directory / "probe.obj", directory / "heap_probe.obj"
                    for label, argv in [
                        ("probe-ir", [args.clang, "--target=x86_64-pc-windows-msvc", "-x", "ir", "-" + opt,
                                      "-c", probe_llvm, "-o", llvm_obj]),
                        ("probe-shim", [args.clang, "--target=x86_64-pc-windows-msvc", "-" + opt,
                                        "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
                                        "-Wall", "-Wextra", "-Werror", "-c",
                                        repo / "tools/language/181/heap_probe.c", "-o", shim_obj]),
                        ("probe-link", [args.linker, "/machine:x64", "/subsystem:windows", "/entry:zil.pe.entry",
                                        "/nodefaultlib", "/timestamp:0", "/out:" + str(probe),
                                        llvm_obj, shim_obj, *args.windows_lib]),
                    ]:
                        result = command(argv, directory / label)
                        if result.returncode:
                            raise RuntimeError(f"{name}/{opt}/{label}: {result.stderr!r}")
                else:
                    linked = command([args.clang, "-" + opt, llvm,
                                      repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                                      "-Wl,--wrap=free", "-o", probe], directory / "probe-link")
                    if linked.returncode:
                        raise RuntimeError(f"{name}/{opt}: probe link failed {linked.stderr!r}")
                row["allocation_probe_sha256"] = digest(probe)
                save(out / "summary.json", report)
                for scenario in scenarios(name):
                    case = directory / scenario["name"]
                    result = run(executable, case / "ordinary", scenario)
                    observe(result, name, scenario, opt, "ordinary", scenario["stdout"], scenario["exit"])
                    if scenario.get("ordinary_only"):
                        continue
                    result = run(probe, case / "allocation", scenario)
                    observe(result, name, scenario, opt, "allocation", scenario["stdout"], scenario["exit"],
                            scenario["allocated"])
                    for fail_at, expected, allocated, exit_code in scenario["failures"]:
                        result = run(probe, case / f"failure-{fail_at}", scenario, fail_at)
                        observe(result, name, scenario, opt, "allocation_failure", expected, exit_code,
                                allocated, fail_at)
        report.update(status="Pass", executions=len(report["observations"]),
                      ordinary_executions=sum(r["mode"] == "ordinary" for r in report["observations"]),
                      instrumented_executions=sum(r["mode"] != "ordinary" for r in report["observations"]),
                      failure_executions=sum(r["mode"] == "allocation_failure" for r in report["observations"]))
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: v for k, v in report.items() if k not in ["observations", "source_hashes", "builds"]},
                     ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Execute the four-module borrowed ByteBuffer consumer and real allocation failures.

Runtime input selects zero to three callback calls. The callback reads one
buffer while its shared view stays live, and grows a distinct buffer through
ordinary helpers. The allocator shim wraps the generated program's actual
realloc/free calls. Oracles are fixed here before execution; no generated C
reimplementation or interpreter supplies the consumer's behavior.
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


def allocation(calls, allocated, failures, peak_bytes):
    return {"alloc_calls": calls, "allocated": allocated, "frees": allocated,
            "failures": failures, "live": 0, "peak_live": allocated,
            "peak_bytes": peak_bytes}


def scenarios():
    rows = []
    for count in range(4):
        prefix = "".join(f"{i}F" for i in range(1, count + 1))
        ordinary = {"stdout": prefix + "TSZ", "exit": 0}
        normal = allocation(count + 1, 1 + int(count > 0), 0, count + 1)
        probes = [{"name": "allocation", "fail_at": 0, **ordinary, "allocation": normal}]
        for fail_at in range(1, count + 2):
            if fail_at == 1:
                output, allocated, peak = "Z", 0, 0
            else:
                output = "".join(f"{i}F" for i in range(1, fail_at - 1)) + "FTSZ"
                allocated, peak = min(2, fail_at - 1), fail_at - 1
            probes.append({"name": f"failure-{fail_at}", "fail_at": fail_at,
                           "stdout": output, "exit": 1,
                           "allocation": allocation(fail_at, allocated, 1, peak)})
        probes.append({"name": "after-last-allocation", "fail_at": count + 2,
                       **ordinary, "allocation": normal})
        rows.append({"name": f"count-{count}", "input": str(count),
                     **ordinary, "probes": probes})
    rows += [{"name": "invalid-input", "input": "9", "stdout": "", "exit": 9, "probes": []},
             {"name": "eof", "input": "", "stdout": "", "exit": 9, "probes": []}]
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
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
    sources = repo / "examples/mainline/borrowed_bytes187"
    report = {"status": "Running", "source_commit": commit, "cli_sha256": digest(cli),
              "sources": {p.name: digest(p) for p in sorted(sources.glob("*.zil"))},
              "allocator_sources": {str(p.relative_to(repo)): digest(p) for p in
                                    [repo / "tools/language/173/allocator_probe.c",
                                     repo / "tools/research/142/allocator_probe.c"]},
              "oracles": scenarios(), "builds": [], "observations": [],
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
            argv = [cli, "build", sources / "main.zil", "--opt", opt,
                    "-o", executable, "--emit-dir", phases, "--json"]
            if args.platform == "win64":
                assert args.wine and args.wine_library_path and args.linker and len(args.windows_lib) == 3
                argv += ["--target", "win64", "--linker", args.linker]
                for library in args.windows_lib:
                    argv += ["--windows-lib", library]
            build = command(argv, directory / "build")
            compiled = json.loads(build.stdout)
            if (build.returncode or compiled["status"] != "Success"
                    or compiled["verification_runs"] != 1 or len(compiled["source_files"]) != 4):
                raise RuntimeError(f"{opt}: {compiled}")
            report["builds"].append({"optimization": opt, "verification_runs": 1,
                                     "llvm_sha256": digest(phases / "llvm.ll"),
                                     "executable_sha256": digest(executable),
                                     "source_files": compiled["source_files"], "module_count": 4})
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
                    ("probe-ir", ["clang", "--target=x86_64-pc-windows-msvc", "-x", "ir", "-" + opt,
                                  "-c", altered, "-o", ir_object]),
                    ("probe-shim", ["clang", "--target=x86_64-pc-windows-msvc", "-" + opt,
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
                linked = command(["clang", "-" + opt, phases / "llvm.ll",
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
        report.update(status="Pass", native_executions=len(report["observations"]),
                      ordinary_executions=sum(r["mode"] == "ordinary" for r in report["observations"]),
                      instrumented_executions=sum(r["mode"] != "ordinary" for r in report["observations"]),
                      actual_allocation_failures=sum(r.get("allocation", {}).get("failures", 0)
                                                     for r in report["observations"]))
    except BaseException as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ("status", "source_commit", "native_executions",
                                             "ordinary_executions", "instrumented_executions",
                                             "actual_allocation_failures")}, indent=2))


if __name__ == "__main__":
    main()

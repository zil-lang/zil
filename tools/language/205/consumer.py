#!/usr/bin/env python3
"""Run the five-module State replacement consumer with independent marker and allocation oracles.

Cancellation is a synchronous source branch, not async cancellation. All mixed
helpers return complete State; preparation errors are Copy arguments in main.
The actual F owns a 204 leaf Store and an independent mutable counter.
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


def expected(mode, fail_at):
    # Fixed x64 source layout: Item(Ticket{i32},i32)=8. Bag is Store header
    # 24 + Ticket4 + counter4 =32; one owned Package capture has 3 flag bytes,
    # so F is40. State is Mark4 + pad4 + F40 + F40 + Tag16 =104 bytes.
    capacity = {"S": 0, "A": 0, "B": 0, "C": 0}
    count = {"A": 0, "B": 0, "C": 0}
    alive = {"A": True, "B": True, "C": False}
    contents = ["A", "B"]
    output = "NANB"
    calls = allocated = frees = peak_live = peak_bytes = failures = 0
    guard = False
    owner = "outer"

    def reserve(name, size):
        nonlocal calls, allocated, failures, peak_live, peak_bytes
        assert size > capacity[name]
        calls += 1
        if calls == fail_at:
            failures += 1
            return False
        if capacity[name] == 0:
            allocated += 1
        capacity[name] = size
        peak_live = max(peak_live, sum(n != 0 for n in capacity.values()))
        peak_bytes = max(peak_bytes, sum(capacity.values()))
        return True

    def free(name):
        nonlocal frees
        if capacity[name]:
            frees += 1
            capacity[name] = 0

    def invoke(name):
        nonlocal output
        assert alive[name]
        if not reserve(name, 8 * (count[name] + 1)):
            return False
        count[name] += 1
        output += name
        return True

    def create_c():
        nonlocal output
        assert not alive["C"]
        alive["C"] = True
        output += "NC"

    def drop(name):
        nonlocal output
        assert alive[name]
        output += {"A": "1", "B": "2", "C": "3"}[name] * count[name]
        free(name)
        output += name.lower()
        alive[name] = False

    def drop_state():
        nonlocal output
        output += "S"
        for name in contents:
            drop(name)

    def report(code):
        assert not any(alive.values())
        assert not any(capacity.values())
        return {"stdout": output, "exit": code,
                "allocation": {"alloc_calls": calls, "allocated": allocated,
                               "frees": frees, "failures": failures, "live": 0,
                               "peak_live": peak_live, "peak_bytes": peak_bytes}}

    def finish(code):
        nonlocal output
        if owner == "owned":
            # replacement is newer than Guard; the popped State is older.
            if alive["C"] and "C" not in contents:
                drop("C")
            if guard:
                output += "Z"
            drop_state()
        else:
            if guard:
                output += "Z"
            drop_state()
        free("S")
        return report(code)

    if not reserve("S", 104):
        return finish(1)
    completed = {1: 1, 2: 1, 7: 3}.get(mode, 0)
    for _ in range(completed):
        for name in ("A", "B"):
            if not invoke(name):
                return finish(1)
    owner = "owned"
    guard = True
    if mode in (3, 4, 5):
        if mode != 3:
            create_c()
        output += "E"
        # Only completed source-order call arguments are unwound here. The
        # target helper is never entered; normal outer Guard cleanup follows.
        if mode == 4:
            drop("C")
        drop_state()
        if mode == 5:
            drop("C")
        output += "Z"
        free("S")
        return report(1)
    create_c()
    if not invoke("C"):
        return finish(1)
    if mode == 2:
        output += "K"
        drop("C")
    else:
        if mode == 6:
            output += "U"
            succeeded = invoke("A")
            # Weak FnOnce consumes this actual FnMut environment before the
            # Copy result is inspected. A handled allocation failure is not
            # equivalent to an early return from main.
            drop("A")
            if not succeeded:
                output += "F"
            output += "VQ"
        else:
            output += "P"
            drop("A")
            output += "Q"
        contents[0] = "C"
    owner = "outer"
    for name in (contents[0], "B", contents[0]):
        if not invoke(name):
            return finish(1)
    if not reserve("S", 208):
        return finish(1)
    output += "L"
    return finish(0)


def scenarios():
    labels = ["reverse-update", "event-update", "synchronous-cancel",
              "label-preparation-failure", "last-argument-failure",
              "reverse-argument-failure", "consume-then-refill", "continue-break-update"]
    rows = []
    for mode, name in enumerate(labels):
        normal = expected(mode, 0)
        probes = [{"name": "allocation", "fail_at": 0, **normal}]
        for fail_at in range(1, normal["allocation"]["alloc_calls"] + 1):
            probes.append({"name": f"failure-{fail_at}", "fail_at": fail_at,
                           **expected(mode, fail_at)})
        probes.append({"name": "after-last-allocation",
                       "fail_at": normal["allocation"]["alloc_calls"] + 1, **normal})
        rows.append({"name": name, "input": str(mode), "stdout": normal["stdout"],
                     "exit": normal["exit"], "probes": probes})
    rows += [{"name": "invalid-input", "input": "9", "stdout": "", "exit": 9, "probes": []},
             {"name": "eof", "input": "", "stdout": "", "exit": 9, "probes": []}]
    return rows


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
    sources = repo / "examples/mainline/state_replacement205"
    frozen_sources = out / "sources"
    frozen_sources.mkdir()
    source_hashes = {}
    for path in sorted(sources.glob("*.zil")):
        data = path.read_bytes()
        (frozen_sources / path.name).write_bytes(data)
        source_hashes[path.name] = hashlib.sha256(data).hexdigest()
    report = {"status": "Running", "source_commit": commit, "cli_sha256": digest(cli),
              "compiler_source_commit": compiler_commit,
              "compiler_build_receipt": str(args.compiler_build_receipt.resolve()),
              "compiler_build_receipt_sha256": digest(args.compiler_build_receipt),
              "compiler_production_matches_checkout": True,
              "design_sha256": digest(repo / "_zil_design.md"),
              "normative_inputs": json.loads((repo / "data/normative-inputs.json").read_text()),
              "sources": source_hashes,
              "allocator_sources": {str(p.relative_to(repo)): digest(p) for p in
                                    [repo / "tools/language/173/allocator_probe.c",
                                     repo / "tools/language/181/heap_probe.c"]},
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

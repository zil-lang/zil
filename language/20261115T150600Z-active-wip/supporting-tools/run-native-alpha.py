#!/usr/bin/env python3
"""Narrow immutable-candidate runner for the existing Alpha native consumers.

Default is a read-only command plan. Root executes under the one build lock.
No repository source, fixture, assertion, compiler flag default or budget is changed.
"""
import argparse
import contextlib
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True

PLAN = Path(__file__).with_name("run-native-alpha-plan.json")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    path = Path(path)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True).strip()


def command(argv, directory, repo, timeout=3600, environment=None, input_bytes=None):
    directory.mkdir(parents=True, exist_ok=False)
    argv = list(map(str, argv))
    row = {"command": argv, "cwd": str(repo), "start": time.time(),
           "controlled_environment": environment or {}, "status": "Running"}
    save(directory / "command.json", row)
    try:
        with (directory / "stdout.log").open("wb") as stdout, (directory / "stderr.log").open("wb") as stderr:
            run = subprocess.run(argv, cwd=repo, input=input_bytes, stdout=stdout, stderr=stderr,
                                 env=dict(os.environ, **(environment or {})), timeout=timeout)
        row.update(status="Completed", exit_code=run.returncode)
    except BaseException as error:
        row.update(status="Fail", error=repr(error), exit_code=None)
        raise
    finally:
        row["end"] = time.time()
        for label in ("stdout", "stderr"):
            path = directory / (label + ".log")
            if path.exists():
                row[label + "_sha256"] = digest(path)
        save(directory / "command.json", row)
    return subprocess.CompletedProcess(argv, run.returncode,
                                       (directory / "stdout.log").read_bytes(),
                                       (directory / "stderr.log").read_bytes())


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def check_codegen(compiled, backend):
    require(compiled.get("status") == "Success", f"compiler status: {compiled}")
    require(compiled.get("verification_runs") == 1, "expected exactly one independent Verify")
    actual = compiled.get("native_codegen") or {}
    require(actual.get("backend") == backend and actual.get("fallback") is None,
            f"backend/fallback mismatch: {actual}")
    if backend == "llvm-api":
        reports = actual.get("object_reports") or []
        require(reports and all(r.get("input_kind") == "builder" and
                               r.get("verify_before_after") is True for r in reports),
                f"missing real verified API builder object evidence: {actual}")


def load_consumer(repo, family):
    relative = {"closures": "tools/language/copy-data-callables/consumer.py",
                "ownership": "tools/language/state-replacement-consumer/consumer.py"}[family]
    spec = importlib.util.spec_from_file_location("alpha_existing_" + family, repo / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, relative


def inject_option(argv, key, value):
    argv = list(map(str, argv))
    while key in argv:
        at = argv.index(key)
        del argv[at:at + 2]
    return argv + [key, str(value)]


def consumer_matrix(args, family, identity):
    """Run original main/scenarios/observe unchanged, adapting only build commands.

    Original probe-link's llvm.ll relink is replaced by another actual CLI build
    from the exact original project, same backend and opt, with the original C
    allocator linked through --clang. Thus API failures execute API objects.
    """
    root = args.out / family
    root.mkdir()
    adapters = []
    for backend in ("clang", "llvm-api"):
        module, relative = load_consumer(args.repo, family)
        original_command = module.command
        builds = {}
        backend_out = root / backend

        def adapted(argv, directory, input_bytes=b"", environment=None):
            argv = list(map(str, argv))
            directory = Path(directory)
            if len(argv) > 1 and Path(argv[0]) == args.cli and argv[1] == "build":
                argv = inject_option(argv, "--backend", backend)
                argv = inject_option(argv, "--clang", args.clang)
                opt = argv[argv.index("--opt") + 1]
                builds[opt] = argv.copy()
                result = original_command(argv, directory, input_bytes, environment)
                require(result.returncode == 0, f"{family}/{backend}/{opt} build failed")
                compiled = json.loads(result.stdout)
                check_codegen(compiled, backend)
                adapters.append({"family": family, "backend": backend, "opt": opt,
                                 "kind": "ordinary", "report": compiled,
                                 "command_receipt": str(directory / "command.json")})
                save(root / "adapter-receipt.json", adapters)
                return result
            if directory.name == "probe-link":
                opt = directory.parent.name
                probe = argv[argv.index("-o") + 1]
                observer = directory.parent / "allocator-observer.o"
                built = original_command(
                    [args.clang, "-std=c11", "-Wall", "-Wextra", "-Werror", "-c",
                     args.repo / "tools/language/composed-store-closures/allocator_probe.c",
                     "-o", observer], directory.parent / "allocator-observer-build")
                require(built.returncode == 0, "original C allocator observer compilation failed")
                captures = directory.parent / "actual-probe-inputs"
                captures.mkdir()
                wrapper = directory.parent / "probe-clang.sh"
                q = shlex.quote
                wrapper.write_text(
                    "#!/bin/sh\nset -eu\nlink=1\ninput=0\n"
                    'for arg in "$@"; do\ncase "$arg" in\n'
                    ' -c|-S|-E|--version) link=0 ;;\n'
                    ' *.o|*.obj|*.ll) input=1 ;;\nesac\ndone\n'
                    'if [ "$link" = 1 ] && [ "$input" = 1 ]; then\n'
                    ' for arg in "$@"; do case "$arg" in *.o|*.obj|*.ll)\n'
                    f'  cp -- "$arg" {q(str(captures))}/ ;; esac; done\n'
                    f' exec {q(args.clang)} "$@" -x none {q(str(observer))}'
                    ' -Wl,--wrap=realloc -Wl,--wrap=free\nfi\n'
                    f'exec {q(args.clang)} "$@"\n')
                wrapper.chmod(0o700)
                probe_argv = inject_option(builds[opt], "-o", probe)
                probe_argv = inject_option(probe_argv, "--emit-dir", directory.parent / "probe-phases")
                probe_argv = inject_option(probe_argv, "--clang", wrapper)
                result = original_command(probe_argv, directory, input_bytes, environment)
                require(result.returncode == 0, f"{family}/{backend}/{opt} actual probe build failed")
                compiled = json.loads(result.stdout)
                check_codegen(compiled, backend)
                files = list(captures.iterdir())
                require(files, "probe link did not preserve its real backend inputs")
                if backend == "llvm-api":
                    require(all(p.suffix in (".o", ".obj") for p in files),
                            "API probe was not linked from actual API objects")
                adapters.append({"family": family, "backend": backend, "opt": opt,
                                 "kind": "actual-backend-allocator-probe", "report": compiled,
                                 "original_relink_command_replaced": argv,
                                 "actual_link_inputs": {str(p): digest(p) for p in files},
                                 "command_receipt": str(directory / "command.json")})
                save(root / "adapter-receipt.json", adapters)
                return result
            return original_command(argv, directory, input_bytes, environment)

        module.command = adapted
        argv = [relative, "--repo", str(args.repo), "--cli", str(args.cli), "--out", str(backend_out),
                "--platform", "linux"]
        if family == "ownership":
            argv += ["--compiler-build-receipt", str(args.compiler_build_receipt),
                     "--compiler-sha256", identity["cli_sha256"], "--clang", args.clang]
        old_argv = sys.argv
        try:
            sys.argv = argv
            with (root / (backend + ".stdout.log")).open("w") as stdout:
                with contextlib.redirect_stdout(stdout):
                    module.main()
        finally:
            sys.argv = old_argv
        report = json.loads((backend_out / "summary.json").read_text())
        require(report["status"] == "Pass" and report["executions"] > 0, "original consumer failed")
        print(json.dumps({"family": family, "backend": backend, "status": "Pass",
                          "executions": report["executions"]}), flush=True)
    require(len(adapters) == 8, "expected four ordinary builds and four actual probe builds")
    return {"family": family, "status": "Pass", "four_zil_configurations": True,
            "original_consumer_sha256": digest(args.repo / relative), "builds": len(adapters),
            "source_and_assertions": "Original consumer main/scenarios/observe reused unchanged",
            "allocation_execution": "Actual same-backend object/IR build with original C observer"}


def stream_matrix(args):
    root = args.out / "project-workflow-matrix"
    root.mkdir()
    # Exact ten stdin/exit/stdout rows from mainline_stream.rs, including binary
    # bytes and both 1000/1001 element boundaries. Independent Rust is retained.
    cases = [(b"", 0, b"0 0 0\n"), (b" \t\n\r\x0b\x0c", 0, b"0 0 0\n"),
             (b"10 20 -3\n", 0, b"3 0 45\n"),
             (b"1 broken 2\t-4 +5\n", 0, b"4 1 23\n"),
             (b"1000000 -1000000 1000001 -1000001 2147483647 -2147483648 000001 +0 -0 + - 9x", 0, b"5 7 1000024\n"),
             (b"\0 3 \xff 4", 0, b"2 2 21\n"),
             (b"\t\r\n\x0b\x0c 7\t8\r\n", 0, b"2 0 33\n"),
             (b"99999999999999999999999999 00000000000000000000000001", 0, b"1 1 2\n"),
             (b"1 " * 1000, 0, b"1000 0 7000\n"), (b"1 " * 1001, 2, b"")]
    app = args.repo / "examples/mainline/stream"
    rows = []
    for backend in ("clang", "llvm-api", "rust"):
        for opt in ("O0", "O2"):
            directory = root / (backend + "-" + opt)
            directory.mkdir()
            binary = directory / "program"
            if backend == "rust":
                argv = ["rustc", app / "main.rs", "--edition=2024", "-C", "opt-level=" + opt[1:],
                        "-C", "overflow-checks=yes", "-D", "warnings", "-o", binary]
            else:
                argv = [args.cli, "build", app / "main.zil", "--backend", backend, "--opt", opt,
                        "--clang", args.clang, "--emit-dir", directory / "phases", "-o", binary, "--json"]
            result = command(argv, directory / "build", args.repo, timeout=180)
            require(result.returncode == 0, f"stream {backend}/{opt} build failed")
            if backend != "rust":
                compiled = json.loads(result.stdout)
                check_codegen(compiled, backend)
                require(len(compiled["source_files"]) == 5, "stream lost part of original module closure")
                llvm = Path(compiled["artifacts"]["llvm"]).read_text()
                require("@getchar(" in llvm and "@putchar(" in llvm and "@malloc(" not in llvm,
                        "stream original C ABI/no malloc assertion failed")
            for index, (stdin, code, stdout) in enumerate(cases):
                result = command([binary], directory / f"case-{index}", args.repo,
                                 timeout=5, input_bytes=stdin)
                (directory / f"case-{index}" / "stdin.bin").write_bytes(stdin)
                require(result.returncode == code and result.stdout == stdout and not result.stderr,
                        f"stream {backend}/{opt}/case-{index}: {result}")
                rows.append({"backend": backend, "optimization": opt, "case": index,
                             "stdin_hex": stdin.hex(), "exit": code, "stdout_hex": stdout.hex()})
    receipt = {"family": "project-workflow", "status": "Pass", "four_zil_configurations": True,
               "zil_executions": 40, "independent_rust_executions": 20, "rows": rows,
               "oracle_source": "crates/zil-driver/tests/mainline_stream.rs",
               "oracle_source_sha256": digest(args.repo / "crates/zil-driver/tests/mainline_stream.rs")}
    save(root / "receipt.json", receipt)
    return receipt


def run_tests(args, selected):
    results = []
    for index, row in enumerate(selected):
        directory = args.out / f"test-{index:02d}-{row['family']}-{row['target']}"
        directory.mkdir()
        artifacts = directory / "artifacts"
        artifacts.mkdir()
        temporary = directory / "tmp"
        temporary.mkdir()
        fixed = args.repo / "out/runs/task-owner-native"
        fixed_used = row["target"] == "task_owner_transport"
        if fixed_used and fixed.exists():
            fixed.rename(directory / "prior-task-owner-native")
        try:
            run = command(row["command"], directory / "cargo", args.repo,
                          environment={"ZIL_TEST_REPORT_DIR": str(artifacts), "TMPDIR": str(temporary)})
        finally:
            if fixed_used and fixed.exists():
                fixed.rename(directory / "task-owner-native")
        output = run.stdout.decode(errors="replace")
        match = re.search(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", output)
        passed = bool(run.returncode == 0 and match and int(match[1]) == row["expected_tests"]
                      and int(match[2]) == 0 and int(match[3]) == 0)
        result = dict(row, status="Pass" if passed else "Fail", exit_code=run.returncode,
                      selected_executed=int(match[1]) if match else 0, log=str(directory))
        results.append(result)
        save(args.out / "test-receipt.json", results)
        print(json.dumps({k: result[k] for k in ("family", "target", "status", "selected_executed")}), flush=True)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("/workspace/zil-alpha-candidate"))
    parser.add_argument("--candidate", help="Full immutable candidate SHA; required for execution")
    parser.add_argument("--cli", type=Path, default=Path("/workspace/.zil-env/target-language-alpha/debug/zil"))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--families", "--family", default="all", help="Comma-separated exact profile families; ct/project also accepted")
    parser.add_argument("--mode", choices=("plan", "build", "tests", "matrix", "all"), default="plan")
    parser.add_argument("--compiler-build-receipt", type=Path)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--lock-held", action="store_true", help="Root already holds language-build.lock")
    args = parser.parse_args()
    args.repo, args.cli = args.repo.resolve(), args.cli.resolve()
    plan = json.loads(PLAN.read_text())
    families = None if args.families == "all" else set(args.families.split(","))
    valid = {r["family"] for r in plan["tests"]} | {"closures", "ownership"}
    require(families is None or families <= valid, f"Unknown families: {families - valid if families else ''}")
    selected = [r for r in plan["tests"] if families is None or r["family"] in families]
    if args.mode == "plan":
        print(json.dumps(dict(plan, tests=selected), ensure_ascii=False, indent=2))
        return 0
    require(args.candidate and re.fullmatch(r"[0-9a-f]{40}", args.candidate), "Supply full --candidate SHA")
    require(args.out is not None, "Execution requires a unique --out under out/runs")
    args.out = args.out.resolve()
    require(git(args.repo, "rev-parse", "HEAD") == args.candidate, "Wrong candidate checkout")
    require(not git(args.repo, "status", "--porcelain"), "Candidate source checkout must be clean")
    args.out.mkdir(parents=True, exist_ok=False)
    lock = open("/workspace/.zil-env/language-build.lock", "a")
    if not args.lock_held:
        fcntl.flock(lock, fcntl.LOCK_EX)
    require(git(args.repo, "rev-parse", "HEAD") == args.candidate and
            not git(args.repo, "status", "--porcelain"), "Candidate changed while waiting for build lock")
    identity = {"source_commit": args.candidate, "git_status": "", "runner_sha256": digest(__file__),
                "plan_sha256": digest(PLAN), "profile_sha256": digest(args.repo / "validation/language-foundation-profile.json")}
    profile = json.loads((args.repo / "validation/language-foundation-profile.json").read_text())
    source_inputs = []
    if args.mode != "build":
        for case in profile["positive_cases"]:
            if families is not None and case["family"] not in families:
                continue
            actual = digest(args.repo / case["source"])
            require(actual == case["source_sha256"], f"Profile source identity changed: {case['source']}")
            for source, expected in case.get("project_sources", {}).items():
                require(digest(args.repo / source) == expected, f"Profile project source changed: {source}")
            source_inputs.append(dict(case, actual_source_sha256=actual))
    report = dict(identity, status="Running", scope="Exact selected consumers, not automatic Alpha acceptance")
    report["source_inputs"] = source_inputs
    save(args.out / "receipt.json", report)
    try:
        if args.mode == "build" or (args.mode == "all" and args.compiler_build_receipt is None):
            run = command(["cargo", "build", "--locked", "-p", "zil-cli", "--features", "llvm-api"],
                          args.out / "compiler-build", args.repo)
            build = dict(identity, exit_code=run.returncode, status="Pass" if run.returncode == 0 else "Fail",
                         command=run.args, cli=str(args.cli))
            if run.returncode == 0:
                build["cli_sha256"] = digest(args.cli)
            args.compiler_build_receipt = args.out / "cli-build-receipt.json"
            save(args.compiler_build_receipt, build)
            require(run.returncode == 0, "Compiler build failed")
        else:
            require(args.compiler_build_receipt is not None, "Supply exact successful --compiler-build-receipt")
            build = json.loads(args.compiler_build_receipt.read_text())
        require(build.get("status") == "Pass" and build.get("exit_code") == 0 and
                build.get("git_status") == "" and build.get("source_commit") == args.candidate and
                build.get("cli_sha256") == digest(args.cli), "Compiler receipt/source/binary identity mismatch")
        identity["cli_sha256"] = digest(args.cli)
        report.update(cli_sha256=identity["cli_sha256"], compiler_build_receipt=str(args.compiler_build_receipt))
        if args.mode in ("tests", "all"):
            report["tests"] = run_tests(args, selected)
        if args.mode in ("matrix", "all"):
            report["matrices"] = []
            for family in ("closures", "ownership", "project-workflow"):
                if families is not None and family not in families:
                    continue
                try:
                    row = stream_matrix(args) if family == "project-workflow" else consumer_matrix(args, family, identity)
                except BaseException as error:
                    row = {"family": family, "status": "Fail", "error": repr(error)}
                report["matrices"].append(row)
                save(args.out / "receipt.json", report)
        checks = report.get("tests", []) + report.get("matrices", [])
        require(args.mode == "build" or checks, "Zero consumers selected/executed")
        report["status"] = "Pass" if all(row["status"] == "Pass" for row in checks) else "Fail"
    except BaseException as error:
        report.update(status="Fail", error=repr(error))
        raise
    finally:
        save(args.out / "receipt.json", report)
    return 0 if report["status"] == "Pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())

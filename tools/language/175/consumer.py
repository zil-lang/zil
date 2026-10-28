#!/usr/bin/env python3
"""Execute dynamic element reads and failure preservation using the real CLI."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    runner_file = repo / "tools/language/173/consumer.py"
    spec = importlib.util.spec_from_file_location("zil_command_receipts", runner_file)
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    command, save, digest = runner.command, runner.save, runner.digest
    out.mkdir(parents=True, exist_ok=False)
    sources = repo / "examples/mainline/impl175"
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    report = {"status": "Running", "source_commit": commit, "cli_sha256": digest(cli),
              "sources": {p.name: digest(p) for p in sorted(sources.glob("*.zil"))},
              "shared_receipt_runner_sha256": digest(runner_file),
              "allocator_probe_sha256": digest(repo / "tools/language/173/allocator_probe.c"),
              "allocator_core_sha256": digest(repo / "tools/research/142/allocator_probe.c"),
              "builds": [], "observations": [], "full_language": "NotSatisfied", "product": "NotRun"}
    save(out / "summary.json", report)
    rows = [("00", b"-", 0, 0, False), ("10", b"aA", 0, 2, True),
            ("30", b"aABC", 0, 4, False), ("32", b"cABC", 0, 4, True),
            ("41", b"bABCD", 0, 5, True), ("44", b"", 9, None, False)]

    def observe(result, input_text, opt, mode, expected_output, expected_exit, allocated=None, fail_at=0):
        if result.returncode != expected_exit or result.stdout != expected_output:
            raise RuntimeError(f"{input_text}/{opt}/{mode}: {result.returncode}/{result.stdout!r}/{result.stderr!r}")
        record = {"input": input_text, "optimization": opt, "mode": mode,
                  "stdout": result.stdout.decode(), "exit_code": result.returncode, "fail_at": fail_at}
        if allocated is not None:
            allocation = json.loads(result.stderr)
            if (allocation["allocated"] != allocated or allocation["frees"] != allocated
                    or allocation["live"] != 0 or allocation["failures"] != int(fail_at != 0)):
                raise RuntimeError(f"responsibility mismatch {input_text}/{opt}/{mode}: {allocation}")
            record["allocation"] = allocation
        elif result.stderr:
            raise RuntimeError(f"unexpected stderr: {result.stderr!r}")
        report["observations"].append(record)
        save(out / "summary.json", report)

    try:
        for opt in ("O0", "O2"):
            folder = out / opt
            program, probe, phases = folder / "program", folder / "probe", folder / "phases"
            built = command([cli, "build", sources / "main.zil", "--opt", opt,
                             "-o", program, "--emit-dir", phases, "--json"], folder / "build")
            data = json.loads(built.stdout)
            if built.returncode or data["status"] != "Success" or data["verification_runs"] != 1:
                raise RuntimeError(f"{opt} build failed: {data}")
            report["builds"].append({"optimization": opt, "verification_runs": 1,
                                     "llvm_sha256": digest(phases / "llvm.ll")})
            save(out / "summary.json", report)
            linked = command(["clang", "-" + opt, phases / "llvm.ll",
                              repo / "tools/language/173/allocator_probe.c", "-Wl,--wrap=realloc",
                              "-Wl,--wrap=free", "-o", probe], folder / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{opt} allocator probe linking failed")
            for input_text, output, exit_code, allocated, failures in rows:
                case = folder / input_text
                normal = command([program], case / "ordinary", input_text.encode())
                observe(normal, input_text, opt, "ordinary", output, exit_code)
                if allocated is None:
                    continue
                normal = command([probe], case / "allocation", input_text.encode(), {"ZIL_TEST_FAIL_AT": "0"})
                observe(normal, input_text, opt, "allocation", output, exit_code, allocated)
                if failures:
                    count = int(input_text[0])
                    for fail_at in range(1, count + 3):
                        if fail_at == 1:
                            expected, acquired = b"", 0
                        elif fail_at <= count + 1:
                            # The failed item's Ticket dies before the earlier
                            # completed prefix, whose order remains ascending.
                            item = fail_at - 2
                            expected = bytes([65 + item]) + bytes(range(65, 65 + item))
                            acquired = fail_at - 1
                        else:
                            # This reserve happens after the original window's
                            # last use. The program rereads the preserved prefix.
                            expected, acquired = output, allocated
                        failed = command([probe], case / f"failure-{fail_at}", input_text.encode(),
                                         {"ZIL_TEST_FAIL_AT": str(fail_at)})
                        observe(failed, input_text, opt, "allocation_failure", expected, 1, acquired, fail_at)
        report.update(status="Pass", native_executions=len(report["observations"]),
                      ordinary_executions=sum(x["mode"] == "ordinary" for x in report["observations"]),
                      instrumented_executions=sum(x["mode"] != "ordinary" for x in report["observations"]))
    except Exception as error:
        report.update(status="Fail", error_type=type(error).__name__, error=str(error))
        save(out / "summary.json", report)
        raise
    save(out / "summary.json", report)
    print(json.dumps({k: report[k] for k in ("status", "source_commit", "native_executions",
                                           "ordinary_executions", "instrumented_executions")}))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Check original CT Γ on four real modules with one immutable CLI.

The two accepted modules must also execute natively. The rejected modules must
hit the exact independent original-definition rule before creating executable
or native-codegen authority. No producer result or fake CT entry is supplied.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def command(argv, directory, timeout):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"argv": [str(item) for item in argv], "status": "Running", "start": time.time()}
    write(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["argv"], capture_output=True, timeout=timeout)
        (directory / "stdout").write_bytes(result.stdout)
        (directory / "stderr").write_bytes(result.stderr)
        receipt.update(status="Completed", exit_code=result.returncode)
        return result
    except subprocess.TimeoutExpired as error:
        (directory / "stdout").write_bytes(error.stdout or b"")
        (directory / "stderr").write_bytes(error.stderr or b"")
        receipt.update(status="Timeout", exit_code=None)
        raise
    finally:
        receipt["end"] = time.time()
        write(directory / "command.json", receipt)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=pathlib.Path, required=True)
    parser.add_argument("--cli", type=pathlib.Path, required=True)
    parser.add_argument("--candidate", required=True, help="Exact source candidate used to build --cli")
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--backend", choices=["clang", "llvm-api"], required=True)
    parser.add_argument("--opt", choices=["O0", "O2"], default="O0")
    parser.add_argument("--timeout", type=int, default=90)
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    inputs = repo / "tests/ct-original-definitions"
    cases = [
        ("no-ct-runtime", 37, None),
        ("unused-mixed", 43, None),
        ("unused-branch-arithmetic", None,
         "original CT binary operator is not justified by its own scalar carrier"),
        ("unused-mixed-kind", None,
         "original CT argument changes its full-ordinal Type/Const/Life kind"),
    ]
    original_hashes = {name: digest(inputs / (name + ".zil")) for name, _, _ in cases}
    summary = {"status": "Running", "candidate": args.candidate, "cli_sha256": digest(cli),
               "backend": args.backend, "optimization": args.opt, "cases": []}
    write(out / "summary.json", summary)
    try:
        for name, expected_exit, expected_message in cases:
            directory = out / name
            directory.mkdir()
            source = directory / "main.zil"
            source.write_bytes((inputs / (name + ".zil")).read_bytes())
            program = directory / "program"
            row = {"case": name, "source_sha256": digest(source), "status": "Running"}
            summary["cases"].append(row)
            write(out / "summary.json", summary)
            if expected_message is None:
                argv = [cli, "build", source, "-o", program, "--backend", args.backend,
                        "--opt", args.opt, "--emit-dir", directory / "phases", "--json"]
            else:
                argv = [cli, "check", source, "--stop-after", "verify",
                        "--emit-dir", directory / "phases", "--json"]
            result = command(argv, directory / "compile", args.timeout)
            report = json.loads(result.stdout)
            write(directory / "report.json", report)
            row.update(result=report["status"], verification_runs=report["verification_runs"],
                       diagnostics=report["diagnostics"])
            if expected_message is None:
                if (result.returncode != 0 or report["status"] != "Success"
                        or report["verification_runs"] != 1 or not program.is_file()
                        or "executable" not in report["artifacts"] or report["native_codegen"] is None):
                    raise AssertionError(f"{name}: native positive did not fully compile: {report}")
                observed = command([program], directory / "run", args.timeout)
                row.update(expected_exit=expected_exit, observed_exit=observed.returncode,
                           stdout_hex=observed.stdout.hex(), stderr_hex=observed.stderr.hex())
                if observed.returncode != expected_exit or observed.stdout or observed.stderr:
                    raise AssertionError(f"{name}: native result differs from the complete expected observation")
            else:
                expected = {"category": "SafeReject", "phase": "ct", "code": "E-CT-ORIGINAL",
                            "message": expected_message}
                write(directory / "expected-diagnostic.json", expected)
                diagnostics = report["diagnostics"]
                if (result.returncode == 0 or report["status"] != "SafeReject"
                        or report["verification_runs"] != 0 or len(diagnostics) != 1
                        or any(diagnostics[0].get(key) != value for key, value in expected.items())
                        or "executable" in report["artifacts"] or program.exists()
                        or report["native_codegen"] is not None):
                    raise AssertionError(f"{name}: wrong rejection or executable authority: {report}")
            row["status"] = "Pass"
            write(out / "summary.json", summary)
        if digest(cli) != summary["cli_sha256"]:
            raise AssertionError("CLI changed during the run")
        if any(digest(inputs / (name + ".zil")) != original_hashes[name] for name, _, _ in cases):
            raise AssertionError("source controls changed during the run")
        summary.update(status="Pass", passed=len(cases), native_runs=2, exact_rejections=2)
    except BaseException as error:
        summary.update(status="Fail", error_type=type(error).__name__, error=str(error))
        raise
    finally:
        write(out / "summary.json", summary)


if __name__ == "__main__":
    main()

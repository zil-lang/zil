#!/usr/bin/env python3
"""Real known-Closure call adapters and adjacent rigid-Type/life controls."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--backend", required=True, choices=["clang", "llvm-api"])
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    summary = {"candidate": args.candidate, "cli_sha256": digest(args.cli),
               "backend": args.backend, "status": "Running", "cases": []}
    cases = [
        ("known-closure-borrowed", None),
        ("negative-closure-whole-type", "original body changes a rigid whole type or lifetime anchor"),
        ("negative-closure-rigid-life", "original projected borrow exceeds its complete source lifetime bound"),
    ]
    try:
        for name, message in cases:
            directory = args.out / name
            directory.mkdir()
            source = args.repo / "tests/fixtures/call-regions" / (name + ".zil")
            executable = directory / "program"
            argv = [str(args.cli), "build" if message is None else "check", str(source),
                    "--emit-dir", str(directory / "phases"), "--json"]
            if message is None:
                argv += ["-o", str(executable), "--backend", args.backend, "--opt", "O0"]
            row = {"case": name, "source_sha256": digest(source), "argv": argv}
            summary["cases"].append(row)
            result = subprocess.run(argv, capture_output=True, timeout=120)
            (directory / "stdout.json").write_bytes(result.stdout)
            (directory / "stderr.log").write_bytes(result.stderr)
            report = json.loads(result.stdout)
            row.update(exit_code=result.returncode, status=report["status"],
                       phase=report.get("phase"), verification_runs=report["verification_runs"],
                       diagnostics=report["diagnostics"])
            if message is None:
                assert result.returncode == 0 and report["status"] == "Success", row
                assert report["verification_runs"] == 1 and executable.is_file(), row
                assert report["native_codegen"] is not None and "executable" in report["artifacts"], row
                run = subprocess.run([str(executable)], capture_output=True, timeout=30)
                row.update(native_exit=run.returncode, native_stdout=run.stdout.hex(), native_stderr=run.stderr.hex())
                assert run.returncode == 0 and not run.stdout and not run.stderr, row
            else:
                expected = {"category": "SafeReject", "phase": "types", "code": "E-SEMANTIC-ORIGINAL", "message": message}
                row["expected"] = expected
                assert result.returncode != 0 and report["status"] == "SafeReject", row
                assert report["verification_runs"] == 0 and len(report["diagnostics"]) == 1, row
                assert all(report["diagnostics"][0].get(key) == value for key, value in expected.items()), row
                assert report["native_codegen"] is None and "executable" not in report["artifacts"] and not executable.exists(), row
            row["pass"] = True
        summary["status"] = "Passed"
    except BaseException:
        summary["status"] = "Failed"
        raise
    finally:
        save(args.out / "receipt.json", summary)


if __name__ == "__main__":
    main()

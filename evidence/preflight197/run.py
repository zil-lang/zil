#!/usr/bin/env python3
"""Reproduce selected 197 input checks without rebuilding the frozen compiler.

Each case runs at most once in its output tree. Copy CASES.json and original
*.zil inputs into a fresh output tree before reproduction. Recorded historical
paths and reports must not be rewritten. This is a check-only preflight.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

SOURCE = "13df78f532f63874b61d56dc6067884dbac0fe9d"
SHA256 = "029543198554cd97118e2f4bc4a5b75ef0767ea5cbbf872f5e4fa075439ab119"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("round", type=Path)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--case", action="append", required=True)
    args = parser.parse_args()
    base = args.round.resolve()
    cli = args.cli.resolve()
    assert digest(cli) == SHA256, "CLI differs from the frozen 194 compiler"
    cases = json.loads((base / "CASES.json").read_text())["cases"]
    assert len(cases) == 12
    selected = [case for case in cases if case["name"] in args.case]
    assert len(selected) == len(args.case), "unknown or repeated case selection"
    prior = list(base.parent.glob("probes*/**/command/command.json"))
    assert len(prior) + len(selected) <= 24, "197 cumulative input-check cap"
    for case in selected:
        folder = base / case["name"]
        command = folder / "command"
        command.mkdir(exist_ok=False)
        argv = [str(cli), "check", str(folder / case["entry"]),
                "--stop-after", "verify", "--json", "--emit-dir",
                str(folder / "emitted")]
        receipt = {
            "argv": argv, "cwd": str(folder), "source_commit": SOURCE,
            "source_status_at_build": "", "cli_sha256": SHA256,
            "cli_bytes": cli.stat().st_size, "phase_limit": "Verify",
            "inputs": {name: digest(folder / name) for name in case["modules"]},
            "start_epoch": time.time(),
            "environment": {"ZIL_CLANG": os.environ.get("ZIL_CLANG")},
            "scope": "197 read-only preflight, no compiler rebuild or production edits",
        }
        try:
            process = subprocess.run(argv, cwd=folder, capture_output=True, timeout=60)
            receipt["exit_code"] = process.returncode
            stdout, stderr = process.stdout, process.stderr
        except subprocess.TimeoutExpired as error:
            receipt.update(exit_code=None, timeout_seconds=60)
            stdout, stderr = error.stdout or b"", error.stderr or b""
        receipt["end_epoch"] = time.time()
        (command / "stdout.json").write_bytes(stdout)
        (command / "stderr.log").write_bytes(stderr)
        receipt["stdout_sha256"] = digest(command / "stdout.json")
        receipt["stderr_sha256"] = digest(command / "stderr.log")
        (command / "command.json").write_text(json.dumps(receipt, indent=2) + "\n")
        report = json.loads(stdout)
        print(json.dumps({"case": case["name"], "status": report["status"],
                          "phase": report["phase"],
                          "verification_runs": report["verification_runs"],
                          "diagnostics": report["diagnostics"]}, ensure_ascii=False),
              flush=True)
    results = []
    for case in cases:
        command = base / case["name"] / "command"
        if not (command / "command.json").exists():
            continue
        receipt = json.loads((command / "command.json").read_text())
        results.append({"name": case["name"], "purpose": case["purpose"],
                        "exit_code": receipt["exit_code"],
                        "report": json.loads((command / "stdout.json").read_text())})
    summary = {"kind": "ReadOnlyPreflightResults", "task": "197",
               "executed_cases": len(results), "native_executions": 0,
               "compiler_source": SOURCE, "cli_sha256": SHA256, "results": results}
    (base / "RESULTS.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Record one real local command, its source identity, log and exit status."""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import time


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


parser = argparse.ArgumentParser()
parser.add_argument("--out", required=True, type=pathlib.Path)
parser.add_argument("--require-tests", action="store_true")
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()
argv = args.command[1:] if args.command[:1] == ["--"] else args.command
if not argv:
    parser.error("a command is required")
args.out.mkdir(parents=True, exist_ok=False)
receipt = {"command": argv, "cwd": str(pathlib.Path.cwd()), "source_commit": git("rev-parse", "HEAD"),
           "crates_tree": git("rev-parse", "HEAD:crates"), "source_status": git("status", "--porcelain", "--", "crates"),
           "start": time.time(), "status": "Running"}
path = args.out / "command.json"
path.write_text(json.dumps(receipt, indent=2) + "\n")
if receipt["source_status"]:
    receipt.update(status="DirtySources", error="commit crates changes before recording a validation run")
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    sys.exit(2)
try:
    with open(args.out / "output.log", "wb") as log:
        result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
    output = (args.out / "output.log").read_bytes()
    tests = [tuple(map(int, row)) for row in re.findall(
        rb"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;", output)]
    receipt.update(status="Pass" if result.returncode == 0 else "Fail", exit_code=result.returncode,
                   log_sha256=hashlib.sha256(output).hexdigest(), end=time.time())
    receipt["seconds"] = receipt["end"] - receipt["start"]
    if tests:
        receipt["tests"] = {"passed": sum(t[0] for t in tests), "failed": sum(t[1] for t in tests),
                            "ignored": sum(t[2] for t in tests), "groups": len(tests)}
    if args.require_tests and (not tests or sum(t[0] for t in tests) == 0 or any(t[1] for t in tests)):
        receipt.update(status="Fail", error="required test run has no passing tests or contains failures")
    receipt["ending_crates_tree"] = git("rev-parse", "HEAD:crates")
    receipt["ending_source_status"] = git("status", "--porcelain", "--", "crates")
    if receipt["ending_crates_tree"] != receipt["crates_tree"] or receipt["ending_source_status"] != receipt["source_status"]:
        receipt["status"] = "SourceChangedDuringRun"
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    sys.exit(0 if receipt["status"] == "Pass" else 1)
except Exception as error:
    receipt.update(status="InfraError", error=str(error), end=time.time())
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    raise

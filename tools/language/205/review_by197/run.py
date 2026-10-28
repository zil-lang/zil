#!/usr/bin/env python3
"""Record a narrowly selected independent Cargo run without changing source."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("label")
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.label or Path(args.label).name != args.label:
        parser.error("label must be a single new directory name")
    repo = Path(__file__).resolve().parents[4]
    root = repo.parents[1] / "run-logs/205-review-by197"
    out = root / args.label
    out.mkdir(parents=True, exist_ok=False)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    status = subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True)
    toolchain = repo.parents[1] / "toolchains178/rust-1.99.1/bin"
    env = os.environ.copy()
    delta = {
        "CARGO_TARGET_DIR": "/tmp/zil205-review-target",
        "CARGO_INCREMENTAL": "0",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "ZIL_TEST_REPORT_DIR": str(out / "cases"),
        "ZIL_CLANG": str(repo.parents[1] / "toolchains178/llvm-17.0.6/usr/lib/llvm-17/bin/clang"),
    }
    env.update(delta)
    env["PATH"] = str(toolchain) + os.pathsep + env.get("PATH", "")
    command = [str(toolchain / "cargo"), "test", "--offline", "--locked", "-j2", *args.arguments]
    receipt = {
        "reviewer": "/root/impl197",
        "source_commit": head,
        "source_clean": not status,
        "source_status": status,
        "command": command,
        "cwd": str(repo),
        "environment_delta": delta,
        "toolchain_path_prefix": str(toolchain),
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    (out / "START.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if status:
        receipt["status"] = "NotRun: source is not clean"
        (out / "command.json").write_text(json.dumps(receipt, indent=2) + "\n")
        raise SystemExit(receipt["status"])
    start = time.monotonic()
    with (out / "stdout.log").open("wb") as stdout, (out / "stderr.log").open("wb") as stderr:
        result = subprocess.run(command, cwd=repo, env=env, stdout=stdout, stderr=stderr)
    receipt.update(exit_code=result.returncode, seconds=time.monotonic() - start,
                   completed_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
    receipt["outputs"] = {}
    for name in ["stdout.log", "stderr.log"]:
        data = (out / name).read_bytes()
        receipt["outputs"][name] = {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    (out / "command.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Record one exact author command without substituting summary claims for output."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("command required")
    args.out.mkdir(parents=True, exist_ok=False)
    receipt = {"command": command, "cwd": str(Path.cwd()), "start": time.time(),
               "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
               "git_status": subprocess.check_output(["git", "status", "--porcelain"], text=True),
               "environment": {k: os.environ.get(k) for k in ["CARGO_TARGET_DIR", "CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG", "ZIL_TEST_REPORT_DIR", "ZIL_CLANG"]},
               "status": "Running"}
    target = args.out / "command.json"
    target.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    try:
        # Open both records before starting work, and preserve output as it is
        # produced. A full temporary filesystem must not erase an entire run
        # merely because the previous recorder wrote its output only at exit.
        with (args.out / "stdout.log").open("wb") as stdout, (args.out / "stderr.log").open("wb") as stderr:
            result = subprocess.run(command, stdout=stdout, stderr=stderr, timeout=args.timeout)
        receipt.update(status="Pass" if result.returncode == 0 else "Fail", exit_code=result.returncode)
    except subprocess.TimeoutExpired:
        receipt.update(status="Timeout", exit_code=None)
        raise
    except OSError as error:
        receipt.update(status="InfrastructureFailure", exit_code=None, infrastructure_error=str(error))
        raise
    finally:
        receipt["end"] = time.time()
        for name in ["stdout.log", "stderr.log"]:
            if (args.out / name).exists():
                receipt[name + "_sha256"] = hashlib.sha256((args.out / name).read_bytes()).hexdigest()
        target.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: receipt[k] for k in ["status", "exit_code", "source_commit"]}))
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()

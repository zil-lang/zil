#!/usr/bin/env python3
"""Run the complete verify-main gate set on the prepared exact source."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
from publish import GATE_IDS, Publication, Refusal, digest, read_json, safe_output, write_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "manifest", "bundle", "repository", "transport-sha", "receipt", "out"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    publisher = Publication(args)
    publisher.receipt_identity(read_json(args.receipt))
    publisher.clean_source()
    output = Path(args.out).resolve()
    output.mkdir(parents=True, exist_ok=True)
    if output.is_relative_to(publisher.repo):
        raise Refusal("verification receipts must be outside the source tree")
    target = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    clang = os.environ["ZIL_CLANG"]
    runner = output.parent
    consumer = runner / "consumer208"
    commands = [
        ("format", ["cargo", "fmt", "--all", "--", "--check"], {}),
        ("workspace", ["cargo", "test", "--locked", "--workspace"], {}),
        ("reference", ["cargo", "test", "--locked", "--workspace", "--no-default-features"],
         {"CARGO_TARGET_DIR": str(runner / "target-reference")}),
        ("native_build", ["cargo", "build", "--locked", "--release", "-p", "zil-cli"], {}),
        ("consumer208", ["python3", "tools/research/208/validate.py", "--repo", ".", "--cli",
                         str(target / "release" / "zil"), "--clang", clang, "--out", str(consumer)], {}),
        ("clippy", ["cargo", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"], {}),
    ]
    assert [item[0] for item in commands] == GATE_IDS
    receipt = {"schema": 1, "stage": "verified", **publisher.local_identity(),
               "source_tree": publisher.git("rev-parse", "HEAD^{tree}").stdout.strip(),
               "status": "Fail", "gates": []}
    write_json(output / "gates.json", receipt)
    failed = False
    for gate, command, overrides in commands:
        print("::group::" + gate, flush=True)
        print("Command: " + " ".join(command), flush=True)
        environment = os.environ.copy()
        environment.update(overrides)
        logfile = output / (gate + ".log")
        with logfile.open("w", encoding="utf-8") as stream:
            proc = subprocess.Popen(command, cwd=publisher.repo, env=environment,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    text=True, encoding="utf-8", errors="replace")
            for line in proc.stdout:
                stream.write(line)
                print(line, end="", flush=True)
            code = proc.wait()
        print("::endgroup::", flush=True)
        receipt["gates"].append({"id": gate, "command": command, "environment_overrides": overrides,
                                 "exit_code": code, "log": logfile.name, "log_sha256": digest(logfile)})
        failed |= code != 0
        write_json(output / "gates.json", receipt)
    publisher.clean_source()
    receipt["status"] = "Fail" if failed else "Pass"
    summary = consumer / "summary.json"
    if summary.is_file():
        (output / "consumer208.json").write_bytes(summary.read_bytes())
    write_json(output / "gates.json", receipt)
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (Refusal, KeyError, ValueError, OSError) as error:
        print("VERIFICATION REFUSED: " + safe_output(str(error)), file=sys.stderr)
        sys.exit(1)

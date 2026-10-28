#!/usr/bin/env python3
"""Run the bounded 180 source/native/raw/Wire suite and adjacent window tests.

This runner uses the caller's installed toolchain. It preserves exact commands,
source identity, stdout/stderr and generated case evidence in a new directory.
It does not claim full-workspace or full-language acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time


def write(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--adjacent", action="store_true")
    args = parser.parse_args()
    repo, out, target = args.repo.resolve(), args.out.resolve(), args.target.resolve()
    out.mkdir(parents=True, exist_ok=False)
    revision = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"], text=True)
    if dirty:
        (out / "dirty.txt").write_text(dirty)
        raise RuntimeError("bounded acceptance requires a committed source checkout")
    selected = ["impl180_store_mut"]
    if args.adjacent:
        selected += ["impl175_store_views", "review175_by176", "impl174_store_fields"]
    command = ["cargo", "test", "--offline", "--locked", "-j2", "-p", "zil-driver"]
    for test in selected:
        command += ["--test", test]
    command += ["--", "--test-threads=2"]
    tools = {}
    for name, version in [("rustc", ["rustc", "-Vv"]), ("cargo", ["cargo", "-V"]),
                          ("clang", [os.environ.get("ZIL_CLANG", "clang"), "--version"])]:
        executable = Path(shutil.which(version[0]) or version[0]).resolve()
        result = subprocess.run(version, capture_output=True, text=True, check=True)
        tools[name] = {"path": str(executable), "sha256": sha(executable), "version": result.stdout}
    record = {"status": "Running", "source_commit": revision,
              "crates_tree": subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD:crates"], text=True).strip(),
              "design_sha256": sha(repo / "_zil_design.md"), "selected": selected,
              "command": command, "cwd": str(repo), "toolchains": tools,
              "target": str(target), "started": time.time(),
              "full_workspace": "NotRun", "full_language": "NotSatisfied", "product": "NotRun"}
    write(out / "summary.json", record)
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), ZIL_TEST_REPORT_DIR=str(out / "cases"))
    try:
        result = subprocess.run(command, cwd=repo, env=environment, capture_output=True, text=True, timeout=1800)
        (out / "stdout.log").write_text(result.stdout)
        (out / "stderr.log").write_text(result.stderr)
        groups = [dict(zip(("passed", "failed", "ignored"), map(int, values)))
                  for values in re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", result.stdout)]
        record.update(exit_code=result.returncode, result_groups=groups,
                      stdout_sha256=sha(out / "stdout.log"), stderr_sha256=sha(out / "stderr.log"))
        if result.returncode or len(groups) != len(selected) or any(g["passed"] == 0 or g["failed"] or g["ignored"] for g in groups):
            raise RuntimeError("bounded test command failed or selected an empty/incomplete group")
        record["status"] = "Pass"
    except BaseException as error:
        record["status"] = "Fail"
        record["error"] = str(error)
        raise
    finally:
        record["finished"] = time.time()
        write(out / "summary.json", record)
    print(json.dumps({k: record[k] for k in ["status", "source_commit", "selected", "result_groups"]}, indent=2))


if __name__ == "__main__":
    main()

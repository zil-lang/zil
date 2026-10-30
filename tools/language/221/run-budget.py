#!/usr/bin/env python3
"""Run exactly three legacy sources against an identified compiler checkout.

The private temporary Cargo package leaves the compiler checkout and its lock
untouched. Compiler inputs are hashed before and after the run. A later run of
this same harness can compare every measured Analyze/raw Verify/Wire boundary.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

CRATES = ("zil-base", "zil-ir", "zil-syntax", "zil-expand", "zil-types", "zil-analyze", "zil-verify")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--compiler-ref", required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()
    repo, scratch, out = args.compiler.resolve(), args.scratch.resolve(), args.out.resolve()
    tool = Path(__file__).resolve().parent
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)

    def git(*words):
        return subprocess.check_output(["git", "-C", str(repo), *words], text=True).strip()

    paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]
    for crate in CRATES:
        paths.extend([f"crates/{crate}/Cargo.toml", f"crates/{crate}/build.rs", f"crates/{crate}/src"])
    changed = git("diff", "--name-only", args.compiler_ref, "--", *paths)
    if changed:
        raise RuntimeError("compiler inputs differ from requested reference: " + changed)
    names = git("ls-files", "--", *paths).splitlines()
    before = {name: sha(repo / name) for name in names}
    inputs = {str(path.relative_to(tool)): sha(path) for path in [tool / "budget.rs", Path(__file__).resolve(), *sorted((tool / "budget-sources").glob("*.zil"))]}
    save(out / "inputs.json", {"compiler": before, "harness": inputs})
    manifest = '[package]\nname = "zil-budget221"\nversion = "0.0.0"\nedition = "2024"\n\n[workspace]\n\n[dependencies]\n'
    for crate in CRATES:
        manifest += f'{crate} = {{ path = {json.dumps(str(repo / "crates" / crate))} }}\n'
    manifest += '\n[[bin]]\nname = "zil-budget221"\npath = "main.rs"\n'
    (scratch / "Cargo.toml").write_text(manifest)
    (scratch / "main.rs").write_bytes((tool / "budget.rs").read_bytes())
    command = ["cargo", "run", "--offline", "--manifest-path", str(scratch / "Cargo.toml"), "--", str(tool / "budget-sources"), str(out)]
    receipt = {"status": "Running", "command": command, "started": time.time(), "compiler_commit": git("rev-parse", "HEAD"), "compiler_ref": git("rev-parse", args.compiler_ref), "compiler_worktree": git("status", "--porcelain"), "harness_inputs": inputs, "scope": "three fixed legacy source budgets; no native, full-workspace or product result"}
    save(out / "RESULT.json", receipt)
    result = subprocess.run(command, capture_output=True)
    (out / "stdout.log").write_bytes(result.stdout)
    (out / "stderr.log").write_bytes(result.stderr)
    after = {name: sha(repo / name) for name in names}
    receipt.update(exit_code=result.returncode, finished=time.time(), compiler_commit_after=git("rev-parse", "HEAD"), compiler_inputs_unchanged=before == after, private_manifest_sha256=sha(scratch / "Cargo.toml"))
    if result.returncode == 0 and before == after:
        receipt["status"] = "Pass"
        if args.compare:
            earlier = json.loads((args.compare / "boundaries.json").read_text())
            current = json.loads((out / "boundaries.json").read_text())
            prior_inputs = json.loads((args.compare / "inputs.json").read_text())["harness"]
            receipt["comparison"] = {"against": str(args.compare.resolve()), "same_harness_inputs": inputs == prior_inputs, "same_boundaries_and_identities": earlier == current}
            if inputs != prior_inputs or earlier != current:
                receipt["status"] = "Fail"
    else:
        receipt["status"] = "Fail"
    save(out / "RESULT.json", receipt)
    print(json.dumps({"status": receipt["status"], "result": str(out / "RESULT.json"), "exit_code": result.returncode}))
    raise SystemExit(0 if receipt["status"] == "Pass" else 1)


if __name__ == "__main__":
    main()

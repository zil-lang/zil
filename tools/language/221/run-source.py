#!/usr/bin/env python3
"""Compile/run 221 author tests against a separately identified combination.

The temporary Cargo package borrows production crates by path. It does not copy
frontend implementations or edit the compiler checkout. Native runs use LLVM
API when enabled; ordinary output and additional allocator-linked output are
recorded separately by the test suite.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

DEPENDENCIES = ("zil-base", "zil-ir", "zil-syntax", "zil-expand", "zil-types", "zil-analyze", "zil-verify", "zil-lower", "zil-backend-llvm", "zil-driver")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--compiler-ref", required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--no-run", action="store_true")
    parser.add_argument("--filter")
    args = parser.parse_args()
    repo, scratch, out = args.compiler.resolve(), args.scratch.resolve(), args.out.resolve()
    tool = Path(__file__).resolve().parent
    author_tree = tool.parents[2]
    suite = author_tree / "crates/zil-driver/tests/impl221_factory_verify.rs"
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)

    def git(*words):
        return subprocess.check_output(["git", "-C", str(repo), *words], text=True).strip()

    paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ":(glob)crates/*/Cargo.toml", ":(glob)crates/*/build.rs", ":(glob)crates/*/src/**"]
    changed = git("diff", "--name-only", args.compiler_ref, "--", *paths)
    if changed:
        raise RuntimeError("production inputs differ from requested compiler reference: " + changed)
    names = git("ls-files", "--", *paths).splitlines()
    before = {name: sha(repo / name) for name in names}
    harness_files = [suite, Path(__file__).resolve(), tool / "ORACLE.json", tool / "SOURCE-MANIFEST.json", *sorted((tool / "cases").rglob("*.zil")), author_tree / "tools/language/173/allocator_probe.c", author_tree / "tools/research/142/allocator_probe.c"]
    harness = {str(path.relative_to(author_tree)): sha(path) for path in harness_files}
    frozen = json.loads((tool / "SOURCE-MANIFEST.json").read_text())
    for path, expected in frozen["files"].items():
        if sha(author_tree / path) != expected:
            raise RuntimeError("source differs from frozen manifest: " + path)
    save(out / "inputs.json", {"compiler": before, "harness": harness})
    manifest = '[package]\nname = "zil-source221"\nversion = "0.0.0"\nedition = "2024"\n\n[workspace]\n\n[features]\ndefault = ["llvm-api"]\nllvm-api = ["zil-driver/llvm-api", "zil-backend-llvm/llvm-api"]\n\n[dependencies]\n'
    for crate in DEPENDENCIES:
        manifest += f'{crate} = {{ path = {json.dumps(str(repo / "crates" / crate))} }}\n'
    manifest += f'\n[lib]\npath = {json.dumps(str(suite))}\n'
    (scratch / "Cargo.toml").write_text(manifest)
    command = ["cargo", "test", "--offline", "--manifest-path", str(scratch / "Cargo.toml"), "--lib"]
    if args.no_run:
        command.append("--no-run")
    if args.filter:
        command.append(args.filter)
    command.extend(["--", "--nocapture", "--test-threads=1"])
    receipt = {"status": "Running", "command": command, "started": time.time(), "compiler_commit": git("rev-parse", "HEAD"), "compiler_ref": git("rev-parse", args.compiler_ref), "compiler_worktree": git("status", "--porcelain"), "author_tree": str(author_tree), "harness_inputs": harness, "mode": "CompileOnly" if args.no_run else "AuthorLimitedSourceRawNative", "native_backend": "llvm-api", "scope": "221 author controls; not independent review or full-language acceptance"}
    save(out / "RESULT.json", receipt)
    environment = dict(os.environ, ZIL_TEST_REPORT_DIR=str(out / "cases"))
    result = subprocess.run(command, capture_output=True, env=environment)
    (out / "stdout.log").write_bytes(result.stdout)
    (out / "stderr.log").write_bytes(result.stderr)
    after = {name: sha(repo / name) for name in names}
    after_harness = {str(path.relative_to(author_tree)): sha(path) for path in harness_files}
    receipt.update(exit_code=result.returncode, finished=time.time(), compiler_commit_after=git("rev-parse", "HEAD"), compiler_inputs_unchanged=before == after, harness_inputs_unchanged=harness == after_harness, private_manifest_sha256=sha(scratch / "Cargo.toml"))
    receipt["status"] = "Pass" if result.returncode == 0 and before == after and harness == after_harness else "Fail"
    if not args.no_run and b"running 0 tests" in result.stdout:
        receipt["status"] = "Fail"
        receipt["reason"] = "zero selected tests are not evidence"
    save(out / "RESULT.json", receipt)
    print(json.dumps({"status": receipt["status"], "mode": receipt["mode"], "result": str(out / "RESULT.json"), "exit_code": result.returncode}))
    raise SystemExit(0 if receipt["status"] == "Pass" else 1)


if __name__ == "__main__":
    main()

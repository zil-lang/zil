#!/usr/bin/env python3
"""Fast local developer verification helper for ZIL.

Default (`--fast`): runs formatting, incremental layout check, index/conformance
checks, crate architecture check, and optional targeted crate/test runs in seconds.

Full (`--full`): runs the complete local quality gate (fmt, clippy, layout --full
--delivery, plan/conformance/indexes, architecture, CI python self-tests, and
workspace tests; automatically selects Clang reference mode when libLLVM-23 is
not configured locally).
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]


def run(cmd: list[str], *, env: dict[str, str] | None = None) -> None:
    print(f"+ {' '.join(cmd)}", flush=True)
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)


def has_llvm_api_dso() -> bool:
    libdir = os.environ.get("ZIL_LLVM_LIB_DIR")
    if not libdir:
        return False
    return (Path(libdir) / "libLLVM-23.so").exists()


def check_architecture() -> None:
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
    )
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tmp:
        tmp.write(metadata)
        tmp_path = Path(tmp.name)
    try:
        run(["node", "tools/check-architecture.mjs", "--metadata", str(tmp_path)])
    finally:
        tmp_path.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--fast",
        action="store_true",
        help="Fast inner-loop check (default): fmt, layout, indexes, architecture, optional targeted tests",
    )
    mode.add_argument(
        "--full",
        action="store_true",
        help="Full local pre-PR gate: fmt, clippy, full layout/plan/conformance/arch, CI tests, workspace tests",
    )
    parser.add_argument(
        "--base",
        default="origin/main",
        help="Git base ref for incremental layout check in --fast mode (default: origin/main)",
    )
    parser.add_argument(
        "-p",
        "--package",
        action="append",
        default=[],
        help="Target crate(s) to test in --fast mode (repeatable)",
    )
    parser.add_argument(
        "--test",
        action="append",
        default=[],
        help="Specific integration test target(s) in --fast mode (repeatable)",
    )
    args = parser.parse_args(argv)
    full = args.full

    run(["cargo", "fmt", "--all", "--", "--check"])

    if full:
        run(["python3", "tools/check-repository-layout.py", "--full", "--delivery"])
        run(["python3", "-m", "unittest", "discover", "-s", "tools", "-p", "test_check_repository_layout.py"])
        run(["python3", "tools/check-plan.py", "--self-test"])
    else:
        run(["python3", "tools/check-repository-layout.py", "--base", args.base])

    run(["python3", "tools/check-conformance.py"])
    run(["python3", "tools/update-indexes.py", "--check"])
    check_architecture()

    if full:
        if shutil.which("zstd"):
            run(["python3", "tools/ci/test_evidence_transport.py"])
        else:
            print("Note: `zstd` not installed locally; skipping tools/ci/test_evidence_transport.py.", flush=True)
        run(["python3", "tools/ci/test_compact_results.py"])
        run(["python3", "tools/ci/test_alpha_acceptance_summary.py"])
        run(["cargo", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"])
        if has_llvm_api_dso():
            run(["cargo", "test", "--locked", "--workspace"])
        else:
            print(
                "Note: ZIL_LLVM_LIB_DIR/libLLVM-23.so not configured; running full Clang reference workspace tests.",
                flush=True,
            )
            run([
                "cargo",
                "test",
                "--locked",
                "--workspace",
                "--exclude",
                "zil-cli",
                "--exclude",
                "zil-llvm-api",
                "--no-default-features",
            ])
            run(["cargo", "test", "--locked", "-p", "zil-cli", "--no-default-features"])
    elif args.package or args.test:
        cmd = ["cargo", "test", "--locked"]
        for pkg in args.package:
            cmd.extend(["-p", pkg])
        for tst in args.test:
            cmd.extend(["--test", tst])
        if not has_llvm_api_dso():
            cmd.append("--no-default-features")
        run(cmd)

    print("dev-check passed.", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

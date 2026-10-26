#!/usr/bin/env python3
"""Build two exact compiler sources in the same path; retain both actual binaries."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time

if not __debug__:
    raise SystemExit("108 compiler identity requires assertions; Python -O/-OO is refused")

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
FOREIGN = "crates/zil-backend-llvm/src/emit/foreign.rs"
TARGET = "x86_64-unknown-linux-gnu"
INHERITED_ENVIRONMENT = ("PATH", "LD_LIBRARY_PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "CARGO_HOME")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dump(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def controlled_environment(inherited, out, rustc, linker):
    # Use an allowlist: encoded flags, wrappers, target/profile overrides and
    # loader interposition variables cannot survive through an unrecorded key.
    env = {key: inherited[key] for key in INHERITED_ENVIRONMENT if key in inherited}
    assert env.get("CARGO_HOME"), "an explicit offline Cargo cache is required"
    env.update(CARGO_TARGET_DIR=str(out / "target"), CARGO_BUILD_BUILD_DIR=str(out / "target"),
               CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1", RUSTC=rustc,
               RUSTFLAGS="", CARGO_ENCODED_RUSTFLAGS="", RUSTC_WRAPPER="", RUSTC_WORKSPACE_WRAPPER="",
               CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_RELEASE_OPT_LEVEL="3",
               CARGO_PROFILE_RELEASE_CODEGEN_UNITS="1", CARGO_PROFILE_RELEASE_LTO="thin",
               CARGO_PROFILE_RELEASE_PANIC="abort", CARGO_PROFILE_RELEASE_INCREMENTAL="false",
               CARGO_PROFILE_RELEASE_STRIP="none", CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS="false",
               CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS="false",
               CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=linker)
    return env


def refuse_unrecorded_cargo_config(source, cargo_home):
    paths = {Path(cargo_home) / name for name in ("config", "config.toml")}
    paths.update(parent / ".cargo" / name for parent in (source, *source.parents)
                 for name in ("config", "config.toml"))
    present = [str(path) for path in sorted(paths) if path.exists() or path.is_symlink()]
    assert not present, "unregistered Cargo configuration: " + repr(present)
    return [str(path) for path in sorted(paths)]


def build(args):
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    try:
        contract = json.loads((HERE / "contract.json").read_text())
        base = contract["production_baseline"]
        candidate = git("rev-parse", args.candidate).decode().strip()
        assert candidate == contract["production_candidate"], "candidate differs from frozen contract"
        changed = git("diff", "--name-only", base, candidate, "Cargo.toml", "Cargo.lock", "crates").decode().splitlines()
        assert [p for p in changed if "/tests/" not in p] == [FOREIGN], changed
        (out / "production.patch").write_bytes(git("diff", base, candidate, "--", FOREIGN))
        source = out / "source"
        source.mkdir()
        archived = git("archive", base, "Cargo.toml", "Cargo.lock", "crates")
        with tarfile.open(fileobj=io.BytesIO(archived)) as archive:
            for member in archive:
                parts = Path(member.name).parts
                assert not Path(member.name).is_absolute() and ".." not in parts
                if member.isdir():
                    continue
                assert member.isfile(), member.name
                path = source / member.name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(archive.extractfile(member).read())
        cargo = str(Path(shutil.which("cargo")).resolve())
        rustc = str(Path(shutil.which("rustc")).resolve())
        linker = str(Path(shutil.which("cc")).resolve())
        env = controlled_environment(os.environ, out, rustc, linker)
        absent_configs = refuse_unrecorded_cargo_config(source, env["CARGO_HOME"])
        result = {"status": "Running", "base": base, "candidate": candidate,
                  "cargo": {"path": cargo, "sha256": digest(cargo)},
                  "rustc": {"path": rustc, "sha256": digest(rustc)}, "arms": {},
                  "linker": {"path": linker, "sha256": digest(linker)}, "target": TARGET,
                  "environment": env, "absent_cargo_config_paths": absent_configs,
                  "environment_policy": "allowlist inherited process environment; explicit empty encoded flags and wrappers; pinned release profile and linker",
                  "source_policy": "same-path Cargo.toml/Cargo.lock/crates snapshot; only production foreign.rs changed; test-only files not compiled"}
        for arm, commit in [("base", base), ("candidate", candidate)]:
            if arm == "candidate":
                (source / FOREIGN).write_bytes(git("show", commit + ":" + FOREIGN))
            files = {str(p.relative_to(source)): digest(p) for p in sorted(source.rglob("*")) if p.is_file()}
            dump(out / (arm + "-source-files.json"), files)
            command = [cargo, "build", "--release", "--locked", "--offline", "--target", TARGET, "-p", "zil-cli"]
            started = time.time()
            with (out / (arm + ".stdout")).open("wb") as stdout, (out / (arm + ".stderr")).open("wb") as stderr:
                process = subprocess.run(command, cwd=source, env=env, stdout=stdout, stderr=stderr, check=False)
            receipt = {"argv": command, "cwd": str(source), "exit": process.returncode,
                       "elapsed_seconds_not_a_comparison": time.time() - started,
                       "source_commit": commit, "source_manifest_sha256": digest(out / (arm + "-source-files.json")),
                       "environment": env, "stdout_sha256": digest(out / (arm + ".stdout")),
                       "stderr_sha256": digest(out / (arm + ".stderr"))}
            dump(out / (arm + ".command.json"), receipt)
            assert process.returncode == 0, arm
            binary = out / ("zil-" + arm)
            shutil.copy2(out / "target" / TARGET / "release/zil", binary)
            result["arms"][arm] = {"commit": commit, "path": str(binary), "sha256": digest(binary),
                                    "source_manifest": str(out / (arm + "-source-files.json")),
                                    "source_manifest_sha256": receipt["source_manifest_sha256"],
                                    "command_receipt": str(out / (arm + ".command.json")),
                                    "command_receipt_sha256": digest(out / (arm + ".command.json"))}
            dump(out / "result.json", result)
            print(json.dumps({"built": arm, "sha256": digest(binary)}), flush=True)
        result["status"] = "Pass"
        dump(out / "result.json", result)
    except BaseException as error:
        dump(out / "failure.json", {"status": "Failed", "type": type(error).__name__, "message": str(error)})
        if (out / "result.json").exists():
            result = json.loads((out / "result.json").read_text())
            result["status"] = "Failed"
            dump(out / "result.json", result)
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--out", required=True)
    build(parser.parse_args())

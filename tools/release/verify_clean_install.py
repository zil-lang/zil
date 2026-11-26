#!/usr/bin/env python3
"""Verify a packaged ZIL Linux x86_64 Alpha release in a clean environment.

Designed to run on a clean machine with NO git checkout, NO Rust toolchain, and
NO Cargo cache — only the downloaded release tarball, SHA256SUMS.txt,
RELEASE-MANIFEST.json, and a system C linker (`clang`).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Any


SAFE_REJECT_SOURCE = """\
package Owned {
    manages buf: ByteBuffer;
}

fn consume(o: Owned) -> i64 {
    return bytes_len(o.buf)
}

fn main() -> i32 {
    let x = Owned { buf: bytes_new() }
    let a = consume(x)
    let b = consume(x)
    if a == b { return 0 }
    return 1
}
"""


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_sha256sums(path: Path) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            raise ValueError(f"malformed SHA256SUMS line: {line!r}")
        digest, name = parts[0], parts[1].lstrip("*")
        mapping[name] = digest
    return mapping


def _clean_env(bin_dir: Path) -> dict[str, str]:
    return {
        "PATH": f"{bin_dir}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "HOME": str(bin_dir.parent),
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
    }


def _run(cmd: list[str], *, env: dict[str, str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=cwd, env=env, text=True, capture_output=True, check=False)


def verify_bundle(
    *,
    archive: Path,
    sha256sums: Path,
    manifest_path: Path,
    run_compiler_checks: bool = True,
) -> tuple[dict[str, Any], list[str]]:
    errors: list[str] = []
    checks: dict[str, Any] = {}

    sums = parse_sha256sums(sha256sums)
    actual_archive_sha = sha256_file(archive)
    actual_manifest_sha = sha256_file(manifest_path)

    if sums.get(archive.name) != actual_archive_sha:
        errors.append(
            f"archive SHA-256 mismatch for {archive.name}: expected {sums.get(archive.name)}, got {actual_archive_sha}"
        )
    if sums.get(manifest_path.name) != actual_manifest_sha:
        errors.append(
            f"manifest SHA-256 mismatch for {manifest_path.name}: expected {sums.get(manifest_path.name)}, got {actual_manifest_sha}"
        )
    checks["checksums"] = "pass" if not errors else "fail"

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    bundle_root_name = str(manifest.get("bundle_root", ""))
    if not bundle_root_name:
        errors.append("RELEASE-MANIFEST.json missing `bundle_root`")

    with tempfile.TemporaryDirectory(prefix="zil-clean-install-") as tmp:
        unpack_dir = Path(tmp) / "install"
        work_dir = Path(tmp) / "work"
        unpack_dir.mkdir(parents=True, exist_ok=True)
        work_dir.mkdir(parents=True, exist_ok=True)

        with tarfile.open(archive, mode="r:gz") as tf:
            for member in tf.getmembers():
                if member.name.startswith("/") or ".." in Path(member.name).parts:
                    raise RuntimeError(f"unsafe tar entry path: {member.name}")
            tf.extractall(unpack_dir, filter="data")

        bundle_dir = unpack_dir / bundle_root_name
        if not bundle_dir.is_dir():
            errors.append(f"extracted bundle root not found: {bundle_dir}")
            return {"status": "fail", "checks": checks, "errors": errors}, errors

        # Verify every packaged file matches RELEASE-MANIFEST.json byte size and SHA-256.
        manifest_files = manifest.get("files") or []
        if not manifest_files:
            errors.append("RELEASE-MANIFEST.json `files` inventory is empty")
        bad_files: list[str] = []
        for entry in manifest_files:
            rel = entry["path"]
            p = bundle_dir / rel
            if not p.is_file():
                bad_files.append(f"missing:{rel}")
                continue
            if p.stat().st_size != int(entry["bytes"]):
                bad_files.append(f"size:{rel}")
            if sha256_file(p) != entry["sha256"]:
                bad_files.append(f"sha256:{rel}")
        if bad_files:
            errors.append(f"packaged file integrity failures: {bad_files}")

        manifest_symlinks = manifest.get("symlinks") or []
        bad_symlinks: list[str] = []
        for entry in manifest_symlinks:
            rel = entry["path"]
            want_target = entry["target"]
            p = bundle_dir / rel
            if not p.is_symlink() or os.readlink(p) != want_target:
                bad_symlinks.append(rel)
        if bad_symlinks:
            errors.append(f"packaged symlink integrity failures: {bad_symlinks}")

        checks["manifest_inventory"] = {
            "status": "pass" if (not bad_files and not bad_symlinks and manifest_files) else "fail",
            "verified_files": len(manifest_files) - len(bad_files),
            "total_files": len(manifest_files),
            "verified_symlinks": len(manifest_symlinks) - len(bad_symlinks),
            "total_symlinks": len(manifest_symlinks),
        }

        inner_manifest = bundle_dir / "RELEASE-MANIFEST.json"
        if not inner_manifest.is_file() or sha256_file(inner_manifest) != actual_manifest_sha:
            errors.append("embedded RELEASE-MANIFEST.json does not match external RELEASE-MANIFEST.json")

        if run_compiler_checks and not errors:
            bin_dir = bundle_dir / "bin"
            zil_exe = str(bin_dir / "zil")
            env = _clean_env(bin_dir)

            # 1. `zil version` in stripped environment (no LD_LIBRARY_PATH / ZIL_LLVM_LIB_DIR / Rust).
            ver = _run([zil_exe, "version"], env=env, cwd=work_dir)
            ver_ok = ver.returncode == 0 and "default backend: llvm-api" in ver.stdout
            checks["zil_version"] = {
                "status": "pass" if ver_ok else "fail",
                "exit_code": ver.returncode,
                "stdout": ver.stdout.strip(),
                "stderr": ver.stderr.strip(),
            }
            if not ver_ok:
                errors.append(f"zil version failed: rc={ver.returncode} out={ver.stdout!r} err={ver.stderr!r}")

            # 2. Build & execute standalone multi-module json-processor project (`Zil.toml`) at O0 and O2 with UTF-16 surrogate pairs.
            json_project = bundle_dir / "examples/everyday-apps/json-processor"
            sample_json = work_dir / "unicode.json"
            sample_json.write_text('{"emoji":"\\uD83D\\uDE00","text":"\\u4E2D\\u6587"}', encoding="utf-8")
            for opt in ("O0", "O2"):
                out_bin = work_dir / f"json-processor-{opt}"
                b = _run(
                    [zil_exe, "build", str(json_project), "-o", str(out_bin), "--opt", opt],
                    env=env,
                    cwd=work_dir,
                )
                if b.returncode != 0:
                    errors.append(f"json-processor build ({opt}) failed: {b.stderr or b.stdout}")
                    checks[f"json_processor_{opt}"] = {"status": "fail", "stage": "build"}
                    continue
                r = _run([str(out_bin), "query", str(sample_json), "emoji"], env=env, cwd=work_dir)
                ok = r.returncode == 0 and r.stdout == '"😀"\n' and r.stderr == ""
                checks[f"json_processor_{opt}"] = {
                    "status": "pass" if ok else "fail",
                    "exit_code": r.returncode,
                    "stdout": r.stdout,
                }
                if not ok:
                    errors.append(
                        f"json-processor run ({opt}) mismatch: rc={r.returncode} out={r.stdout!r} err={r.stderr!r}"
                    )

            # 3. Build & execute standalone multi-module async-task-scheduler project (`Zil.toml`) at O2.
            # Also copy it without Zil.lock into work_dir and verify `zil lock` works in a clean env.
            async_project = bundle_dir / "examples/everyday-apps/async-task-scheduler"
            copied_async = work_dir / "fresh-async-project"
            copied_async.mkdir(parents=True, exist_ok=True)
            for item in async_project.iterdir():
                if item.is_file() and item.name != "Zil.lock":
                    (copied_async / item.name).write_bytes(item.read_bytes())
            lk = _run([zil_exe, "lock", str(copied_async)], env=env, cwd=work_dir)
            lk_ok = lk.returncode == 0 and (copied_async / "Zil.lock").is_file()
            checks["zil_lock"] = {"status": "pass" if lk_ok else "fail", "exit_code": lk.returncode}
            if not lk_ok:
                errors.append(f"zil lock failed: rc={lk.returncode} out={lk.stdout!r} err={lk.stderr!r}")

            async_bin = work_dir / "async-scheduler-O2"
            ba = _run(
                [zil_exe, "build", str(copied_async), "-o", str(async_bin), "--opt", "O2"],
                env=env,
                cwd=work_dir,
            )
            if ba.returncode != 0:
                errors.append(f"async-task-scheduler build failed: {ba.stderr or ba.stdout}")
                checks["async_task_scheduler_O2"] = {"status": "fail", "stage": "build"}
            else:
                ra = _run([str(async_bin)], env=env, cwd=work_dir)
                ok = (
                    ra.returncode == 0
                    and "SUMMARY completed=4 cancelled=1 " in ra.stdout
                    and ra.stderr == ""
                )
                checks["async_task_scheduler_O2"] = {
                    "status": "pass" if ok else "fail",
                    "exit_code": ra.returncode,
                    "stdout_lines": len(ra.stdout.splitlines()),
                }
                if not ok:
                    errors.append(
                        f"async-task-scheduler run failed: rc={ra.returncode} out={ra.stdout!r} err={ra.stderr!r}"
                    )

            # 4. SafeReject negative control via `zil check --json`.
            neg_file = work_dir / "use_after_move.zil"
            neg_file.write_text(SAFE_REJECT_SOURCE, encoding="utf-8")
            nc = _run([zil_exe, "check", str(neg_file), "--json"], env=env, cwd=work_dir)
            neg_ok = False
            parsed_status = ""
            try:
                payload = json.loads(nc.stdout)
                parsed_status = str(payload.get("status", ""))
                neg_ok = nc.returncode != 0 and parsed_status == "SafeReject"
            except Exception:  # noqa: BLE001
                neg_ok = False
            checks["safe_reject_control"] = {
                "status": "pass" if neg_ok else "fail",
                "exit_code": nc.returncode,
                "reported_status": parsed_status,
            }
            if not neg_ok:
                errors.append(
                    f"SafeReject control failed: rc={nc.returncode} status={parsed_status!r} out={nc.stdout!r}"
                )

    report = {
        "status": "pass" if not errors else "fail",
        "source_commit": manifest.get("source_commit", ""),
        "source_tree": manifest.get("source_tree", ""),
        "archive_name": archive.name,
        "archive_sha256": actual_archive_sha,
        "manifest_sha256": actual_manifest_sha,
        "checks": checks,
        "errors": errors,
    }
    return report, errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--sha256sums", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--out-json", type=Path, required=True)
    args = parser.parse_args(argv)

    report, errors = verify_bundle(
        archive=args.archive.resolve(),
        sha256sums=args.sha256sums.resolve(),
        manifest_path=args.manifest.resolve(),
        run_compiler_checks=True,
    )
    args.out_json.parent.mkdir(parents=True, exist_ok=True)
    args.out_json.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())

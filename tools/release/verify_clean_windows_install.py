#!/usr/bin/env python3
"""Verify a packaged ZIL Windows x86_64 Alpha release in a clean environment.

Designed to run on a clean Windows runner with NO git checkout, NO Rust toolchain,
and NO Cargo cache — only the downloaded release zip, SHA256SUMS.txt,
RELEASE-MANIFEST.json, and a system C linker (`clang.exe`).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
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


def _clean_windows_env(bin_dir: Path, work_dir: Path) -> dict[str, str]:
    # Preserve only core Windows OS variables and system tool paths (no Cargo/Rust/ZIL_LLVM_LIB_DIR).
    system_root = os.environ.get("SystemRoot", r"C:\Windows")
    path_parts = [str(bin_dir), f"{system_root}\\System32", system_root]
    clang_found = shutil.which("clang")
    if clang_found:
        clang_dir = str(Path(clang_found).resolve().parent)
        if clang_dir not in path_parts:
            path_parts.append(clang_dir)
    llvm_default = Path(r"C:\Program Files\LLVM\bin")
    if llvm_default.is_dir() and str(llvm_default) not in path_parts:
        path_parts.append(str(llvm_default))

    env: dict[str, str] = {
        "PATH": os.pathsep.join(path_parts),
        "SystemRoot": system_root,
        "WINDIR": system_root,
        "TEMP": str(work_dir),
        "TMP": str(work_dir),
        "USERPROFILE": str(work_dir),
        "ComSpec": os.environ.get("ComSpec", f"{system_root}\\System32\\cmd.exe"),
    }
    for key in (
        "SystemDrive",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "LOCALAPPDATA",
        "APPDATA",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "VCToolsInstallDir",
        "WindowsSdkDir",
        "INCLUDE",
        "LIB",
    ):
        val = os.environ.get(key)
        if val:
            env[key] = val
    return env


def _run(cmd: list[str], *, env: dict[str, str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, check=False)


def verify_windows_bundle(
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

    with tempfile.TemporaryDirectory(prefix="zil-win64-clean-install-") as tmp:
        unpack_dir = Path(tmp) / "install"
        work_dir = Path(tmp) / "work"
        unpack_dir.mkdir(parents=True, exist_ok=True)
        work_dir.mkdir(parents=True, exist_ok=True)

        with zipfile.ZipFile(archive, mode="r") as zf:
            for member in zf.namelist():
                if member.startswith("/") or member.startswith("\\") or ".." in Path(member).parts:
                    raise RuntimeError(f"unsafe zip entry path: {member}")
            zf.extractall(unpack_dir)

        bundle_dir = unpack_dir / bundle_root_name
        if not bundle_dir.is_dir():
            errors.append(f"extracted bundle root not found: {bundle_dir}")
            return {"status": "fail", "checks": checks, "errors": errors}, errors

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

        checks["manifest_inventory"] = {
            "status": "pass" if (not bad_files and manifest_files) else "fail",
            "verified_files": len(manifest_files) - len(bad_files),
            "total_files": len(manifest_files),
        }

        inner_manifest = bundle_dir / "RELEASE-MANIFEST.json"
        if not inner_manifest.is_file() or sha256_file(inner_manifest) != actual_manifest_sha:
            errors.append("embedded RELEASE-MANIFEST.json does not match external RELEASE-MANIFEST.json")

        if run_compiler_checks and not errors:
            bin_dir = bundle_dir / "bin"
            zil_exe = str(bin_dir / "zil.exe")
            env = _clean_windows_env(bin_dir, work_dir)

            # 1. `zil.exe version` in stripped environment (resolves side-by-side LLVM-C.dll).
            ver = _run([zil_exe, "version"], env=env, cwd=work_dir)
            ver_out = ver.stdout.decode("utf-8", errors="replace")
            ver_err = ver.stderr.decode("utf-8", errors="replace")
            expected_semver = str(manifest.get("release_name", "")).lstrip("v")
            ver_ok = (
                ver.returncode == 0
                and "default backend: llvm-api" in ver_out
                and (not expected_semver or expected_semver in ver_out)
            )
            checks["zil_version"] = {
                "status": "pass" if ver_ok else "fail",
                "exit_code": ver.returncode,
                "stdout": ver_out.strip(),
                "stderr": ver_err.strip(),
            }
            if not ver_ok:
                errors.append(f"zil.exe version failed: rc={ver.returncode} out={ver_out!r} err={ver_err!r}")

            # 2. Build & execute standalone json-processor (`Zil.toml`) at O0 & O2 via default llvm-api backend.
            json_project = bundle_dir / "examples/everyday-apps/json-processor"
            sample_json = work_dir / "unicode.json"
            sample_json.write_text('{"emoji":"\\uD83D\\uDE00","text":"\\u4E2D\\u6587"}', encoding="utf-8")
            for opt in ("O0", "O2"):
                out_bin = work_dir / f"json-processor-{opt}.exe"
                b = _run(
                    [zil_exe, "build", str(json_project), "-o", str(out_bin), "--opt", opt],
                    env=env,
                    cwd=work_dir,
                )
                if b.returncode != 0:
                    err_txt = (b.stderr or b.stdout).decode("utf-8", errors="replace")
                    errors.append(f"json-processor build ({opt}) failed: {err_txt}")
                    checks[f"json_processor_{opt}"] = {"status": "fail", "stage": "build"}
                    continue
                r = _run([str(out_bin), "query", str(sample_json), "emoji"], env=env, cwd=work_dir)
                ok = r.returncode == 0 and r.stdout == '"😀"\n'.encode("utf-8") and r.stderr == b""
                checks[f"json_processor_{opt}"] = {
                    "status": "pass" if ok else "fail",
                    "exit_code": r.returncode,
                    "stdout": r.stdout.decode("utf-8", errors="replace"),
                }
                if not ok:
                    errors.append(
                        f"json-processor run ({opt}) mismatch: rc={r.returncode} out={r.stdout!r} err={r.stderr!r}"
                    )

            # 3. Copy async-task-scheduler without Zil.lock, run `zil.exe lock`, build O2, and run `demo`.
            async_project = bundle_dir / "examples/everyday-apps/async-task-scheduler"
            copied_async = work_dir / "fresh-async-project"
            copied_async.mkdir(parents=True, exist_ok=True)
            for item in async_project.iterdir():
                if item.is_file() and item.name != "Zil.lock":
                    (copied_async / item.name).write_bytes(item.read_bytes())

            lk = _run([zil_exe, "lock", str(copied_async)], env=env, cwd=work_dir)
            if lk.returncode != 0 or not (copied_async / "Zil.lock").is_file():
                errors.append(
                    f"zil.exe lock failed: rc={lk.returncode} err={lk.stderr.decode('utf-8', errors='replace')}"
                )
                checks["zil_lock"] = {"status": "fail", "exit_code": lk.returncode}
            else:
                checks["zil_lock"] = {"status": "pass", "exit_code": 0}

            async_bin = work_dir / "async-scheduler-O2.exe"
            ab = _run(
                [zil_exe, "build", str(copied_async), "-o", str(async_bin), "--opt", "O2"],
                env=env,
                cwd=work_dir,
            )
            if ab.returncode != 0:
                errors.append(
                    f"async-task-scheduler build failed: {(ab.stderr or ab.stdout).decode('utf-8', errors='replace')}"
                )
                checks["async_task_scheduler_O2"] = {"status": "fail", "stage": "build"}
            else:
                ar = _run([str(async_bin)], env=env, cwd=work_dir)
                ar_out = ar.stdout.decode("utf-8", errors="replace")
                ok = (
                    ar.returncode == 0
                    and "SUMMARY completed=4 cancelled=1 " in ar_out
                    and ar.stderr == b""
                )
                checks["async_task_scheduler_O2"] = {
                    "status": "pass" if ok else "fail",
                    "exit_code": ar.returncode,
                    "stdout_lines": len(ar_out.splitlines()),
                }
                if not ok:
                    errors.append(
                        f"async-task-scheduler run failed: rc={ar.returncode} out={ar_out!r} err={ar.stderr!r}"
                    )

            # 4. SafeReject negative control must fail before codegen.
            neg_src = work_dir / "safe_reject.zil"
            neg_src.write_text(SAFE_REJECT_SOURCE, encoding="utf-8")
            neg = _run([zil_exe, "check", str(neg_src), "--json"], env=env, cwd=work_dir)
            neg_out = neg.stdout.decode("utf-8", errors="replace")
            neg_ok = False
            parsed_status = ""
            if neg.returncode != 0:
                try:
                    payload = json.loads(neg_out)
                    parsed_status = str(payload.get("status", ""))
                    neg_ok = parsed_status == "SafeReject"
                except Exception:  # noqa: BLE001
                    neg_ok = False
            checks["safe_reject_control"] = {
                "status": "pass" if neg_ok else "fail",
                "exit_code": neg.returncode,
                "reported_status": parsed_status,
            }
            if not neg_ok:
                errors.append(f"SafeReject control failed: rc={neg.returncode} out={neg_out!r}")

    receipt = {
        "schema_version": 1,
        "status": "pass" if not errors else "fail",
        "target": "x86_64-pc-windows-msvc",
        "release_name": manifest.get("release_name"),
        "source_commit": manifest.get("source_commit"),
        "source_tree": manifest.get("source_tree"),
        "archive_name": archive.name,
        "archive_sha256": actual_archive_sha,
        "manifest_sha256": actual_manifest_sha,
        "checks": checks,
        "errors": errors,
    }
    return receipt, errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--sha256sums", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--out-receipt", type=Path, default=None)
    parser.add_argument("--skip-compiler-checks", action="store_true")
    args = parser.parse_args(argv)

    receipt, errors = verify_windows_bundle(
        archive=args.archive.resolve(),
        sha256sums=args.sha256sums.resolve(),
        manifest_path=args.manifest.resolve(),
        run_compiler_checks=not args.skip_compiler_checks,
    )
    rendered = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if args.out_receipt is not None:
        args.out_receipt.parent.mkdir(parents=True, exist_ok=True)
        args.out_receipt.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())

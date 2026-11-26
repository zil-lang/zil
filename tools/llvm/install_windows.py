#!/usr/bin/env python3
"""Install the pinned official LLVM 23.1.4 Windows x86_64 MSVC SDK into a prefix.

Downloads (or consumes a pre-downloaded) clang+llvm-23.1.4-x86_64-pc-windows-msvc.tar.zst,
verifies its exact byte size and SHA-256 against tools/llvm/release-windows-x86_64.json,
extracts the pinned LLVM-C DLL/import library and clang/lld-link binaries, verifies
LLVMGetVersion == (23, 1, 4) directly via ctypes on Windows, and writes install-receipt.json.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _download(url: str, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": "zil-llvm-installer/1.0"})
    with urllib.request.urlopen(req, timeout=600) as resp, dst.open("wb") as out:
        shutil.copyfileobj(resp, out, length=1 << 20)


def _find_tar() -> str:
    # Prefer Windows System32 bsdtar (supports --zstd natively on Windows 11/Server 2022+).
    if os.name == "nt":
        sys_tar = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "tar.exe"
        if sys_tar.is_file():
            return str(sys_tar)
    found = shutil.which("tar")
    if not found:
        raise RuntimeError("tar executable not found on PATH")
    return found


def _extract_selected(archive: Path, prefix: Path, members: list[str]) -> list[str]:
    tar_exe = _find_tar()
    zstd_exe = shutil.which("zstd")

    # Try bsdtar native zstd decompression first (no external zstd binary required).
    cmd_native = [
        tar_exe,
        "-xf",
        str(archive),
        "--strip-components=1",
        "-C",
        str(prefix),
        *members,
    ]
    proc = subprocess.run(cmd_native, capture_output=True, text=True, timeout=600)
    if proc.returncode == 0:
        return cmd_native

    # Fallback: explicit zstd decompressor pipe/filter.
    if zstd_exe:
        cmd_zstd = [
            tar_exe,
            "-I",
            f"{zstd_exe} -d",
            "-xf",
            str(archive),
            "--strip-components=1",
            "-C",
            str(prefix),
            *members,
        ]
        proc2 = subprocess.run(cmd_zstd, capture_output=True, text=True, timeout=600)
        if proc2.returncode == 0:
            return cmd_zstd
        raise RuntimeError(
            f"tar extraction failed (native rc={proc.returncode}: {proc.stderr.strip()}; "
            f"zstd rc={proc2.returncode}: {proc2.stderr.strip()})"
        )

    raise RuntimeError(f"tar extraction failed (rc={proc.returncode}): {proc.stderr.strip()}")


def _verify_dll_version(dll_path: Path, expected_version: str) -> list[int]:
    expected = [int(p) for p in expected_version.split(".")]
    lib = ctypes.CDLL(str(dll_path))
    get_ver = lib.LLVMGetVersion
    get_ver.argtypes = [
        ctypes.POINTER(ctypes.c_uint),
        ctypes.POINTER(ctypes.c_uint),
        ctypes.POINTER(ctypes.c_uint),
    ]
    get_ver.restype = None
    maj, min_v, pat = ctypes.c_uint(0), ctypes.c_uint(0), ctypes.c_uint(0)
    get_ver(ctypes.byref(maj), ctypes.byref(min_v), ctypes.byref(pat))
    got = [int(maj.value), int(min_v.value), int(pat.value)]
    if got != expected:
        raise RuntimeError(f"LLVM-C.dll reported version {got}, expected {expected}")
    return got


def install_windows_llvm(prefix: Path, archive: Path | None, release: dict[str, Any]) -> dict[str, Any]:
    prefix = prefix.resolve()
    prefix.mkdir(parents=True, exist_ok=True)
    root = str(release["archive_root"])
    required_files: list[str] = list(release["required_files"])
    resource_dirs: list[str] = list(release.get("resource_dirs") or [])
    members = [f"{root}/{rel}" for rel in required_files + resource_dirs]

    tmp_dir_ctx = None
    if archive is None:
        tmp_dir_ctx = tempfile.TemporaryDirectory(prefix="zil-llvm-win64-dl-")
        archive = Path(tmp_dir_ctx.name) / Path(release["url"]).name
        _download(str(release["url"]), archive)

    try:
        actual_bytes = archive.stat().st_size
        actual_sha = sha256_file(archive)
        if actual_bytes != int(release["archive_bytes"]) or actual_sha != str(release["archive_sha256"]):
            raise ValueError(
                f"archive integrity mismatch: bytes={actual_bytes} (expected {release['archive_bytes']}), "
                f"sha256={actual_sha} (expected {release['archive_sha256']})"
            )

        extract_cmd = _extract_selected(archive, prefix, members)
    finally:
        if tmp_dir_ctx is not None:
            tmp_dir_ctx.cleanup()

    installed_files: dict[str, dict[str, Any]] = {}
    for rel in required_files:
        full = prefix / rel
        if not full.is_file():
            raise RuntimeError(f"expected extracted LLVM file missing: {full}")
        installed_files[rel] = {
            "bytes": full.stat().st_size,
            "sha256": sha256_file(full),
        }

    dll_version: list[int] | None = None
    clang_banner = ""
    if os.name == "nt":
        dll_version = _verify_dll_version(prefix / "bin/LLVM-C.dll", str(release["version"]))
        proc = subprocess.run(
            [str(prefix / "bin/clang.exe"), "--version"],
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        clang_banner = proc.stdout.splitlines()[0].strip() if proc.stdout.splitlines() else ""
        if str(release["version"]) not in clang_banner:
            raise RuntimeError(f"clang.exe version banner mismatch: {clang_banner!r}")

    receipt: dict[str, Any] = {
        "schema_version": 1,
        "target": "x86_64-pc-windows-msvc",
        "llvm_version": str(release["version"]),
        "host": platform.platform(),
        "archive": {
            "url": str(release["url"]),
            "bytes": int(release["archive_bytes"]),
            "sha256": str(release["archive_sha256"]),
        },
        "extract_command": extract_cmd,
        "installed_files": installed_files,
        "dll_version_verified": dll_version,
        "clang_banner": clang_banner,
    }
    (prefix / "install-receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--archive", type=Path, default=None)
    parser.add_argument(
        "--release-manifest",
        type=Path,
        default=HERE / "release-windows-x86_64.json",
    )
    args = parser.parse_args(argv)

    release = json.loads(args.release_manifest.read_text(encoding="utf-8"))
    pinned = (HERE / "version.txt").read_text(encoding="utf-8").strip()
    if release.get("version") != pinned:
        raise ValueError(f"release manifest version {release.get('version')!r} != version.txt {pinned!r}")

    receipt = install_windows_llvm(args.prefix, args.archive, release)
    print(json.dumps(receipt, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())

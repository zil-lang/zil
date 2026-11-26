#!/usr/bin/env python3
"""Build a reproducible Windows x86_64 Alpha release archive for ZIL.

Produces:
  - zil-<version>-windows-x86_64.zip (deterministic zip archive)
  - RELEASE-MANIFEST.json (exact commit/tree, toolchain versions, and per-file SHA-256)
  - SHA256SUMS.txt (SHA-256 checksums of archive and manifest)
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


FIXED_ZIP_DATE = (1980, 1, 1, 0, 0, 0)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def _cmd_banner(cmd: list[str]) -> str:
    try:
        out = subprocess.check_output(cmd, text=True, stderr=subprocess.STDOUT, timeout=15)
        return out.splitlines()[0].strip() if out.splitlines() else ""
    except Exception:  # noqa: BLE001
        return ""


def _copy_tree(src: Path, dst: Path) -> None:
    for root, dirs, files in os.walk(src):
        dirs.sort()
        files.sort()
        rel_root = Path(root).relative_to(src)
        target_dir = dst / rel_root
        target_dir.mkdir(parents=True, exist_ok=True)
        for name in files:
            src_file = Path(root) / name
            dst_file = target_dir / name
            shutil.copy2(src_file, dst_file)


def _create_deterministic_zip(stage_root: Path, bundle_name: str, out_archive: Path) -> None:
    entries: list[Path] = []
    for root, dirs, files in os.walk(stage_root):
        dirs.sort()
        files.sort()
        rel_root = Path(root).relative_to(stage_root)
        for name in files:
            entries.append(rel_root / name)
    entries.sort(key=lambda p: p.as_posix())

    out_archive.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out_archive, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
        for rel in entries:
            full = stage_root / rel
            arcname = f"{bundle_name}/{rel.as_posix()}"
            info = zipfile.ZipInfo(filename=arcname, date_time=FIXED_ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3  # Unix external_attr format for cross-host reproducibility
            mode = 0o755 if rel.suffix.lower() in (".exe", ".dll", ".bat", ".cmd") else 0o644
            info.external_attr = (0o100000 | mode) << 16
            zf.writestr(info, full.read_bytes())


def package_windows_release(
    *,
    repo: Path,
    zil_exe: Path,
    llvm_dll: Path,
    out_dir: Path,
    version: str,
    source_commit: str,
    source_tree: str,
    clang_bin: str = "clang",
    lock_examples: bool = False,
) -> dict[str, Any]:
    llvm_version = (repo / "tools/llvm/version.txt").read_text(encoding="utf-8").strip()
    zil_exe = zil_exe.resolve(strict=True)
    llvm_dll = llvm_dll.resolve(strict=True)

    bundle_name = f"zil-{version}-windows-x86_64"
    archive_name = f"{bundle_name}.zip"

    with tempfile.TemporaryDirectory(prefix="zil-win64-release-stage-") as tmp:
        stage = Path(tmp) / bundle_name
        (stage / "bin").mkdir(parents=True, exist_ok=True)
        (stage / "docs/guide").mkdir(parents=True, exist_ok=True)
        (stage / "examples/everyday-apps").mkdir(parents=True, exist_ok=True)

        # 1. Ship zil.exe and pinned LLVM-C.dll side-by-side in bin/ so Windows
        #    resolves LLVM-C.dll automatically with zero environment variables.
        zil_dst = stage / "bin/zil.exe"
        shutil.copy2(zil_exe, zil_dst)

        dll_dst = stage / "bin/LLVM-C.dll"
        shutil.copy2(llvm_dll, dll_dst)

        # 2. Bundle docs, license, and all 8 everyday multi-module ZIL apps.
        for doc_rel in ("docs/guide/build.md", "docs/guide/supported-language.md"):
            src_doc = repo / doc_rel
            if not src_doc.is_file():
                raise FileNotFoundError(f"missing required guide doc: {src_doc}")
            shutil.copy2(src_doc, stage / doc_rel)

        for top_file in ("README.md", "LICENSE", "LICENSE-MIT", "LICENSE-APACHE"):
            src_top = repo / top_file
            if src_top.is_file():
                shutil.copy2(src_top, stage / top_file)

        _copy_tree(repo / "examples/everyday-apps", stage / "examples/everyday-apps")

        if lock_examples:
            for app_dir in sorted((stage / "examples/everyday-apps").iterdir()):
                if (app_dir / "Zil.toml").is_file():
                    subprocess.run([str(zil_dst), "lock", str(app_dir)], check=True)

        # 3. Record exact file inventory before writing RELEASE-MANIFEST.json.
        files_inventory: list[dict[str, Any]] = []
        for root, dirs, files in os.walk(stage):
            dirs.sort()
            files.sort()
            for name in files:
                p = Path(root) / name
                rel = p.relative_to(stage).as_posix()
                files_inventory.append(
                    {
                        "path": rel,
                        "bytes": p.stat().st_size,
                        "sha256": sha256_file(p),
                    }
                )

        manifest: dict[str, Any] = {
            "schema_version": 1,
            "release_name": version,
            "target": "x86_64-pc-windows-msvc",
            "archive_name": archive_name,
            "bundle_root": bundle_name,
            "source_commit": source_commit,
            "source_tree": source_tree,
            "toolchain": {
                "llvm_version": llvm_version,
                "llvm_dll": "bin/LLVM-C.dll",
                "llvm_dll_sha256": sha256_file(dll_dst),
                "rustc_banner": _cmd_banner(["rustc", "--version"]),
                "clang_banner": _cmd_banner([clang_bin, "--version"]),
            },
            "runtime_prerequisites": [
                "Windows 10/11 or Windows Server 2022/2025 (x86_64) with Universal CRT",
                "System C linker driver (`clang.exe` on PATH + Windows SDK/MSVC UCRT import libs) for final native PE linking",
            ],
            "files": files_inventory,
        }

        manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
        (stage / "RELEASE-MANIFEST.json").write_bytes(manifest_bytes)

        out_dir.mkdir(parents=True, exist_ok=True)
        out_archive = out_dir / archive_name
        _create_deterministic_zip(stage, bundle_name, out_archive)

        out_manifest = out_dir / "RELEASE-MANIFEST.json"
        out_manifest.write_bytes(manifest_bytes)

        archive_sha = sha256_file(out_archive)
        manifest_sha = sha256_file(out_manifest)
        sums_text = f"{archive_sha}  {archive_name}\n{manifest_sha}  RELEASE-MANIFEST.json\n"
        (out_dir / "SHA256SUMS.txt").write_text(sums_text, encoding="utf-8")

        return {
            "status": "pass",
            "archive": str(out_archive),
            "archive_bytes": out_archive.stat().st_size,
            "archive_sha256": archive_sha,
            "manifest_sha256": manifest_sha,
            "file_count": len(files_inventory) + 1,
            "source_commit": source_commit,
            "source_tree": source_tree,
        }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--zil-exe", type=Path, required=True)
    parser.add_argument("--llvm-dll", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--version", default="v1.0.0-alpha.1")
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--source-tree", required=True)
    parser.add_argument("--clang-bin", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--lock-examples", action="store_true")
    args = parser.parse_args(argv)

    result = package_windows_release(
        repo=args.repo.resolve(),
        zil_exe=args.zil_exe,
        llvm_dll=args.llvm_dll,
        out_dir=args.out_dir.resolve(),
        version=args.version,
        source_commit=args.source_commit,
        source_tree=args.source_tree,
        clang_bin=args.clang_bin,
        lock_examples=args.lock_examples,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())

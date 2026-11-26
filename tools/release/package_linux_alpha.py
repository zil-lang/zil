#!/usr/bin/env python3
"""Build a reproducible Linux x86_64 Alpha release archive for ZIL.

Produces:
  - zil-<version>-linux-x86_64.tar.gz (deterministic tar+gzip archive)
  - RELEASE-MANIFEST.json (exact commit/tree, toolchain versions, and per-file SHA-256)
  - SHA256SUMS.txt (SHA-256 checksums of archive and manifest)
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Any


WRAPPER_SCRIPT = """#!/usr/bin/env sh
set -eu
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
LIB_DIR="$ROOT_DIR/lib"
export ZIL_LLVM_LIB_DIR="${ZIL_LLVM_LIB_DIR:-$LIB_DIR}"
if [ -n "${LD_LIBRARY_PATH:-}" ]; then
  export LD_LIBRARY_PATH="$LIB_DIR:$LD_LIBRARY_PATH"
else
  export LD_LIBRARY_PATH="$LIB_DIR"
fi
exec "$SCRIPT_DIR/zil-bin" "$@"
"""


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


def _create_deterministic_tar_gz(stage_root: Path, bundle_name: str, out_archive: Path) -> None:
    entries: list[Path] = []
    for root, dirs, files in os.walk(stage_root):
        dirs.sort()
        files.sort()
        rel_root = Path(root).relative_to(stage_root)
        if str(rel_root) != ".":
            entries.append(rel_root)
        for name in files:
            entries.append(rel_root / name)

    out_archive.parent.mkdir(parents=True, exist_ok=True)
    with out_archive.open("wb") as raw_fh:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw_fh, mtime=0) as gz_fh:
            with tarfile.open(fileobj=gz_fh, mode="w", format=tarfile.GNU_FORMAT) as tf:
                root_info = tarfile.TarInfo(name=bundle_name)
                root_info.type = tarfile.DIRTYPE
                root_info.mode = 0o755
                root_info.mtime = 0
                root_info.uid = 0
                root_info.gid = 0
                root_info.uname = "root"
                root_info.gname = "root"
                tf.addfile(root_info)

                for rel in entries:
                    full = stage_root / rel
                    arcname = f"{bundle_name}/{rel.as_posix()}"
                    st = full.lstat()
                    info = tarfile.TarInfo(name=arcname)
                    info.mtime = 0
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "root"
                    if stat.S_ISDIR(st.st_mode):
                        info.type = tarfile.DIRTYPE
                        info.mode = 0o755
                        tf.addfile(info)
                    elif stat.S_ISLNK(st.st_mode):
                        info.type = tarfile.SYMTYPE
                        info.mode = 0o777
                        info.linkname = os.readlink(full)
                        tf.addfile(info)
                    elif stat.S_ISREG(st.st_mode):
                        info.type = tarfile.REGTYPE
                        info.mode = 0o755 if (st.st_mode & 0o111) else 0o644
                        info.size = st.st_size
                        with full.open("rb") as fobj:
                            tf.addfile(info, fobj)
                    else:
                        raise RuntimeError(f"unsupported file type in release stage: {full}")


def package_release(
    *,
    repo: Path,
    zil_bin: Path,
    llvm_lib_dir: Path,
    out_dir: Path,
    version: str,
    source_commit: str,
    source_tree: str,
    clang_bin: str = "clang",
) -> dict[str, Any]:
    llvm_version = (repo / "tools/llvm/version.txt").read_text(encoding="utf-8").strip()
    parts = llvm_version.split(".")
    llvm_major, llvm_minor = parts[0], parts[1]
    llvm_soname = f"libLLVM.so.{llvm_major}.{llvm_minor}"
    llvm_link_name = f"libLLVM-{llvm_major}.so"
    soname_candidate = llvm_lib_dir / llvm_soname
    link_candidate = llvm_lib_dir / llvm_link_name
    if soname_candidate.exists():
        llvm_dso_src = soname_candidate.resolve(strict=True)
    else:
        llvm_dso_src = link_candidate.resolve(strict=True)
    zil_bin = zil_bin.resolve(strict=True)

    bundle_name = f"zil-{version}-linux-x86_64"
    archive_name = f"{bundle_name}.tar.gz"

    with tempfile.TemporaryDirectory(prefix="zil-release-stage-") as tmp:
        stage = Path(tmp) / bundle_name
        (stage / "bin").mkdir(parents=True, exist_ok=True)
        (stage / "lib").mkdir(parents=True, exist_ok=True)
        (stage / "docs/guide").mkdir(parents=True, exist_ok=True)
        (stage / "examples/everyday-apps").mkdir(parents=True, exist_ok=True)

        # 1. Ship real ELF binary + self-contained launcher wrapper.
        zil_elf_dst = stage / "bin/zil-bin"
        shutil.copy2(zil_bin, zil_elf_dst)
        os.chmod(zil_elf_dst, 0o755)

        wrapper_dst = stage / "bin/zil"
        wrapper_dst.write_text(WRAPPER_SCRIPT, encoding="utf-8")
        os.chmod(wrapper_dst, 0o755)

        # 2. Bundle pinned libLLVM shared library under its runtime ELF SONAME
        # (`libLLVM.so.<major>.<minor>`) plus compatibility symlink `libLLVM-<major>.so`.
        llvm_dst = stage / "lib" / llvm_soname
        shutil.copy2(llvm_dso_src, llvm_dst)
        os.chmod(llvm_dst, 0o644)
        (stage / "lib" / llvm_link_name).symlink_to(llvm_soname)

        # 3. Bundle docs, license, and the 8 everyday multi-module ZIL apps.
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

        # 4. Record exact file & symlink inventory before writing RELEASE-MANIFEST.json.
        files_inventory: list[dict[str, Any]] = []
        symlinks_inventory: list[dict[str, str]] = []
        for root, dirs, files in os.walk(stage):
            dirs.sort()
            files.sort()
            for name in files:
                p = Path(root) / name
                rel = p.relative_to(stage).as_posix()
                if p.is_symlink():
                    symlinks_inventory.append({"path": rel, "target": os.readlink(p)})
                else:
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
            "target": "x86_64-unknown-linux-gnu",
            "archive_name": archive_name,
            "bundle_root": bundle_name,
            "source_commit": source_commit,
            "source_tree": source_tree,
            "toolchain": {
                "llvm_version": llvm_version,
                "llvm_dso": f"lib/{llvm_soname}",
                "llvm_dso_link": f"lib/{llvm_link_name}",
                "llvm_dso_sha256": sha256_file(llvm_dst),
                "rustc_banner": _cmd_banner(["rustc", "--version"]),
                "clang_banner": _cmd_banner([clang_bin, "--version"]),
            },
            "runtime_prerequisites": [
                "Linux x86_64 (glibc >= 2.39 / Ubuntu 24.04 or compatible)",
                "System C linker driver (`clang` on PATH) for final native ELF linking",
                "System shared libraries: libzstd1, libxml2, zlib1g",
            ],
            "files": files_inventory,
            "symlinks": symlinks_inventory,
        }

        manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
        (stage / "RELEASE-MANIFEST.json").write_bytes(manifest_bytes)

        out_dir.mkdir(parents=True, exist_ok=True)
        out_archive = out_dir / archive_name
        _create_deterministic_tar_gz(stage, bundle_name, out_archive)

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
    parser.add_argument("--zil-bin", type=Path, required=True)
    parser.add_argument("--llvm-lib-dir", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--version", default="alpha")
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--source-tree", required=True)
    parser.add_argument("--clang-bin", default=os.environ.get("ZIL_CLANG", "clang"))
    args = parser.parse_args(argv)

    result = package_release(
        repo=args.repo.resolve(),
        zil_bin=args.zil_bin,
        llvm_lib_dir=args.llvm_lib_dir,
        out_dir=args.out_dir.resolve(),
        version=args.version,
        source_commit=args.source_commit,
        source_tree=args.source_tree,
        clang_bin=args.clang_bin,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())

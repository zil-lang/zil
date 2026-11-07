#!/usr/bin/env python3
"""Collect immutable review receipts; omit executables and object files.

This is evidence packaging, not a language acceptance check. The archive keeps
failed attempts and exact source identities alongside the final successful run.
"""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, out = args.root.resolve(), args.out.resolve()
    if out == root or root in out.parents:
        parser.error("archive destination must be outside the source directory")
    out.mkdir(parents=True, exist_ok=True)
    archive = out / "review-results.zip"
    included, omitted = [], []
    with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for path in sorted(root.rglob("*")):
            if path.is_symlink():
                raise RuntimeError(f"unexpected symlink: {path}")
            if not path.is_file():
                continue
            data = path.read_bytes()
            relative = path.relative_to(root).as_posix()
            row = {"path": relative, "bytes": len(data), "sha256": digest(data)}
            executable = data.startswith((b"\x7fELF", b"MZ", b"!<arch>\n"))
            object_file = path.suffix.lower() in {".o", ".obj", ".a", ".so", ".exe", ".dll", ".lib"}
            if executable or object_file:
                row["reason"] = "execution artifact; hash retained, bytes excluded"
                omitted.append(row)
                continue
            entry = zipfile.ZipInfo(relative, (1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            target.writestr(entry, data)
            included.append(row)
        manifest = {"included": included, "omitted_execution_artifacts": omitted}
        target.writestr("ARCHIVE-MANIFEST.json", json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    with zipfile.ZipFile(archive) as target:
        bad = target.testzip()
        if bad is not None:
            raise RuntimeError(f"archive CRC mismatch: {bad}")
        for row in included:
            if digest(target.read(row["path"])) != row["sha256"]:
                raise RuntimeError(f"archive hash mismatch: {row['path']}")
    result = {
        "scope": "evidence packaging only",
        "status": "ArchivedAndVerified",
        "archive": archive.name,
        "bytes": archive.stat().st_size,
        "sha256": digest(archive.read_bytes()),
        "included_files": len(included),
        "included_bytes": sum(row["bytes"] for row in included),
        "omitted_execution_artifacts": len(omitted),
        "omitted_execution_bytes": sum(row["bytes"] for row in omitted),
        "full_manifest": "ARCHIVE-MANIFEST.json inside archive",
    }
    (out / "MANIFEST.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

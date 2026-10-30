#!/usr/bin/env python3
"""Inventory phase-native evidence and release it only after archival.

The workflow must gate release on successful receipt and native-artifact uploads.
Release independently verifies that the inventory still matches every file.
"""
import hashlib
import json
from pathlib import Path
import shutil
import stat
import sys


def inventory(root, phase):
    evidence = root / "native-evidence" / phase
    if evidence.is_symlink() or evidence != evidence.resolve():
        raise SystemExit("Native evidence path must be canonical and not a symlink")
    if not evidence.exists():
        return []
    if not evidence.is_dir():
        raise SystemExit("Native evidence path must be a directory")
    files = []
    folded_paths = {}
    for path in sorted(evidence.rglob("*")):
        if path.is_symlink():
            raise SystemExit("Native evidence must not contain symlinks")
        relative = path.relative_to(evidence).as_posix()
        folded = relative.casefold()
        if folded in folded_paths:
            raise SystemExit("Native artifact paths must not collide when case-folded: "
                             + folded_paths[folded] + " and " + relative)
        folded_paths[folded] = relative
        if path.is_dir():
            continue
        if not path.is_file():
            raise SystemExit("Native evidence must contain only regular files")
        digest = hashlib.sha256()
        size = 0
        with path.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                digest.update(chunk)
                size += len(chunk)
        files.append({"path": relative, "mode": stat.S_IMODE(path.stat().st_mode),
                      "bytes": size, "sha256": digest.hexdigest()})
    return files


def main():
    if len(sys.argv) != 4 or sys.argv[1] not in ("capture", "release"):
        raise SystemExit("usage: native_evidence.py capture|release RUNNER_TEMP workspace|reference")
    action, value, phase = sys.argv[1:]
    root = Path(value)
    if not root.is_absolute() or root != root.resolve() or root.is_symlink():
        raise SystemExit("Runner temporary root must be absolute and canonical")
    if phase not in ("workspace", "reference"):
        raise SystemExit("Unsupported evidence phase")
    manifest = root / "receipts" / f"{phase}-native-manifest.json"
    current = {"phase": phase, "root": str(root / "native-evidence" / phase),
               "files": inventory(root, phase)}
    if action == "capture":
        manifest.parent.mkdir(parents=True, exist_ok=True)
        manifest.write_text(json.dumps(current, indent=2) + "\n")
        print(f"native_file_count={len(current['files'])}")
    else:
        if json.loads(manifest.read_text()) != current:
            raise SystemExit("Native evidence changed after the archived inventory")
        evidence = root / "native-evidence" / phase
        if evidence.exists():
            shutil.rmtree(evidence)
        print(f"Released {len(current['files'])} inventoried native files after archival")
        print("Free bytes after cleanup:", shutil.disk_usage(root).free)


if __name__ == "__main__":
    main()

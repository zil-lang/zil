#!/usr/bin/env python3
"""Preserve every 101 raw file, then verify each archived byte stream."""
from __future__ import annotations
import argparse, gzip, hashlib, json, stat, tarfile, time
from pathlib import Path

DIRECTORIES = [
    "zil-101-author", "zil-101-inspect", "zil-101-diag001", "zil-101-cal001",
    "zil-101-campaign001", "zil-101-confirm-prepare001", "zil-101-confirm-prepare002",
    "zil-101-controller-integrity001", "zil-101-controller-integrity002",
    "zil-101-controller-integrity003",
]
LOGS = [
    "zil-101-diag001-prepare.log", "zil-101-diag001-sample.log",
    "zil-101-cal001-sample.log", "zil-101-campaign001-sample.log",
    "zil-101-confirm-prepare001.log", "zil-101-confirm-prepare002.log",
    "zil-101-confirm-prepare002-sample.log", "zil-101-confirm-prepare002-analysis.log",
]


def sha(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def files(root):
    result = []
    for name in DIRECTORIES:
        folder = root / name
        assert folder.is_dir() and not folder.is_symlink(), folder
        for path in folder.rglob("*"):
            assert not path.is_symlink(), path
            if path.is_file():
                assert stat.S_ISREG(path.stat().st_mode), path
                result.append(path)
    for name in LOGS:
        path = root / name
        assert path.is_file() and not path.is_symlink(), path
        result.append(path)
    result.sort(key=lambda path: path.relative_to(root).as_posix())
    assert len(result) == len(set(result))
    return result


def create(root, out):
    out.mkdir(parents=True, exist_ok=True)
    archive, manifest = out / "raw.tar.gz", out / "raw-files.json"
    assert not archive.exists() and not manifest.exists()
    paths = files(root)
    rows = []
    last = time.monotonic()
    with archive.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.PAX_FORMAT) as target:
                for index, path in enumerate(paths, 1):
                    name = path.relative_to(root).as_posix()
                    size, mode = path.stat().st_size, stat.S_IMODE(path.stat().st_mode)
                    digest = sha(path)
                    info = tarfile.TarInfo(name)
                    info.size, info.mode, info.mtime = size, mode, 0
                    with path.open("rb") as source:
                        target.addfile(info, source)
                    rows.append(dict(path=name, bytes=size, mode=mode, sha256=digest))
                    if time.monotonic() - last >= 25:
                        print("archive", index, "/", len(paths), flush=True)
                        last = time.monotonic()
    write(manifest, dict(schema=1, scope="All listed raw originals, including failures and superseded protocols; no post-result sampling",
        roots=DIRECTORIES, logs=LOGS, excluded_rebuildable_cache="zil-101-unit-target; tested unit binaries and source/command/output originals are retained under zil-101-author",
        files=len(rows), bytes=sum(row["bytes"] for row in rows), entries=rows))
    verify(out, expected_sources=paths)


def verify(out, expected_sources=None):
    archive, manifest = out / "raw.tar.gz", out / "raw-files.json"
    data = json.loads(manifest.read_text())
    expected = {row["path"]: row for row in data["entries"]}
    assert len(expected) == len(data["entries"]) == data["files"]
    seen, total = set(), 0
    with tarfile.open(archive, "r|gz") as source:
        for member in source:
            assert member.isfile() and member.name not in seen and member.name in expected
            assert not Path(member.name).is_absolute() and ".." not in Path(member.name).parts
            row = expected[member.name]
            assert member.size == row["bytes"] and member.mode == row["mode"]
            stream = source.extractfile(member)
            assert hashlib.file_digest(stream, "sha256").hexdigest() == row["sha256"], member.name
            seen.add(member.name)
            total += member.size
    assert seen == set(expected) and total == data["bytes"]
    if expected_sources is not None:
        # Source files remain in place; the archive is not used to delete evidence.
        assert all(path.is_file() for path in expected_sources)
    receipt = dict(status="Pass", scope="Every raw archive member and exact manifest set verified; no performance/production admission implied",
        archive=dict(file=archive.name, bytes=archive.stat().st_size, sha256=sha(archive)),
        manifest=dict(file=manifest.name, bytes=manifest.stat().st_size, sha256=sha(manifest)),
        files=len(seen), uncompressed_bytes=total, script_sha256=sha(__file__), source_originals_deleted_by_this_tool=False)
    write(out / "raw-archive.json", receipt)
    print(json.dumps(receipt), flush=True)


if __name__ == "__main__":
    if not __debug__:
        raise RuntimeError("Evidence validation requires assertions enabled")
    p = argparse.ArgumentParser()
    p.add_argument("command", choices=["create", "verify"])
    p.add_argument("--root", type=Path, default=Path("/tmp"))
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.command == "create":
        create(a.root.resolve(), a.out.resolve())
    else:
        verify(a.out.resolve())

#!/usr/bin/env python3
"""Convert an already verified frozen ZIP to standard solid tar.xz."""
import argparse
import hashlib
import json
import lzma
import os
from pathlib import Path
import stat
import sys
import tarfile
import zipfile

# This entry point may run from inside the manifested extracted directory.
sys.dont_write_bytecode = True
import compact_evidence as compact


CHUNK = 1024 * 1024


class HashingReader:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()
        self.count = 0

    def read(self, size=-1):
        data = self.stream.read(size)
        self.digest.update(data)
        self.count += len(data)
        return data


def convert(source, output, max_bytes=50_000_000, progress=None):
    source, output = Path(source), Path(output)
    pending = output.with_name(output.name + ".partial")
    if source.is_symlink() or not source.is_file():
        raise ValueError("conversion source must be a regular frozen ZIP")
    for path in (output, pending):
        if path.exists() or path.is_symlink():
            raise FileExistsError("output or partial archive already exists")
    if max_bytes <= 0:
        raise ValueError("archive byte cap must be positive")
    before = compact.fingerprint(source)
    source_sha = compact.file_digest(source)
    index = compact.verify(source)
    output.parent.mkdir(parents=True, exist_ok=True)
    records = []
    with zipfile.ZipFile(source) as original:
        entries = original.infolist()
        names = {entry.filename for entry in entries}
        for entry in entries:
            # ZIPs made by writestr may omit the Unix file-type bits. Explicit
            # symlink/device/directory metadata must never be normalized away.
            kind = stat.S_IFMT(entry.external_attr >> 16)
            if entry.is_dir() or kind not in (0, stat.S_IFREG):
                raise ValueError("ZIP symlink or nonregular member: " + entry.filename)
            if any(parent.as_posix() in names
                   for parent in compact.safe_path(entry.filename).parents):
                raise ValueError("ZIP member is also a parent directory: " + entry.filename)
    with zipfile.ZipFile(source) as original, pending.open("xb") as raw:
        # One compressor spans all tar members. No decoded member tree exists.
        with lzma.LZMAFile(raw, "w", preset=6) as compressed:
            with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.PAX_FORMAT) as archive:
                entries = original.infolist()
                for position, entry in enumerate(entries, 1):
                    compact.safe_path(entry.filename)
                    member = tarfile.TarInfo(entry.filename)
                    member.size = entry.file_size
                    member.mode = 0o644
                    member.mtime = 0
                    with original.open(entry, "r") as stream:
                        reader = HashingReader(stream)
                        archive.addfile(member, reader)
                        if reader.read(1) or reader.count != entry.file_size:
                            raise ValueError("ZIP member length changed: " + entry.filename)
                    records.append((entry.filename, reader.count, reader.digest.hexdigest()))
                    if progress and position % 1000 == 0:
                        progress({"phase": "compress", "members": position, "total": len(entries)})
    if compact.fingerprint(source) != before:
        raise ValueError("source changed during compression; partial retained")

    with zipfile.ZipFile(source) as original, lzma.open(pending, "rb") as decoded:
        count = 0
        with tarfile.open(fileobj=decoded, mode="r|") as archive:
            for member in archive:
                if count >= len(records) or not member.isfile():
                    raise ValueError("unexpected tar member")
                name, size, sha = records[count]
                if member.name != name or member.size != size:
                    raise ValueError("tar member mapping differs from original ZIP")
                digest = hashlib.sha256()
                with original.open(name, "r") as left, archive.extractfile(member) as right:
                    while True:
                        a, b = left.read(CHUNK), right.read(CHUNK)
                        if a != b:
                            raise ValueError("tar member bytes differ: " + name)
                        if not a:
                            break
                        digest.update(b)
                if digest.hexdigest() != sha:
                    raise ValueError("tar member digest differs: " + name)
                count += 1
                if progress and count % 1000 == 0:
                    progress({"phase": "compare", "members": count, "total": len(records)})
        # Drain the XZ footer/check even if tar stopped at its end marker.
        while decoded.read(CHUNK):
            pass
        if count != len(records):
            raise ValueError("tar member is missing")
    if compact.fingerprint(source) != before or compact.file_digest(source) != source_sha:
        raise ValueError("source changed during comparison; partial retained")
    size = pending.stat().st_size
    if size > max_bytes:
        raise ValueError(f"archive has {size} bytes, exceeds byte cap {max_bytes}; partial retained")
    os.link(pending, output)
    pending.unlink()
    return {"integrity": "Pass", "path": str(output.resolve()), "bytes": size,
            "sha256": compact.file_digest(output), "source_sha256": source_sha,
            "members_compared_byte_for_byte": len(records),
            "decoded_member_bytes": sum(row[1] for row in records),
            "statistics": index["statistics"], "compression": "standard tar.xz, solid LZMA preset 6",
            "member_content_changed": False, "test_execution": "NotRun by converter"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    converting = commands.add_parser("convert")
    converting.add_argument("source", type=Path)
    converting.add_argument("output", type=Path)
    converting.add_argument("--max-bytes", type=int, default=50_000_000)
    args = parser.parse_args()
    def progress(value):
        print(json.dumps(value), file=sys.stderr, flush=True)
    print(json.dumps(convert(args.source, args.output, args.max_bytes, progress), indent=2))


if __name__ == "__main__":
    main()

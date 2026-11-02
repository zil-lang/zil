#!/usr/bin/env python3
"""Lossless, bounded CI evidence transport when artifact storage is unavailable.

This preserves files, modes and paths, including native programs. It never uploads
Cargo/LLVM caches, changes test outcomes or deletes source evidence. Log transport
is a recovery channel: the integrator must retrieve and verify it before merging.
"""
import argparse
import base64
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile


FORMAT = "zil-ci-evidence-v1"
MARKER = "ZIL_CI_EVIDENCE_V1"
ROOTS = ("receipts", "native-evidence", "consumer209",
         "llvm-23.1.4/install-receipt.json", "llvm-23.1.4/installation-logs")
MAX_ARCHIVE = 256 * 1024 * 1024
MAX_EXPANDED = 16 * 1024 * 1024 * 1024
MAX_INDEX = 64 * 1024 * 1024
MAX_FILES = 250_000
CHUNK = 4096


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe_path(name):
    require(isinstance(name, str) and name and len(name) <= 4096,
            "invalid evidence path")
    path = PurePosixPath(name)
    require(path.parts and not path.is_absolute() and path.as_posix() == name
            and all(part not in (".", "..") for part in path.parts)
            and not any(char in name for char in ("\\", ":", "\0", "\n", "\r")),
            "unsafe evidence path")
    return path


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def inventory(root):
    require(root.is_absolute() and root == root.resolve() and not root.is_symlink(),
            "runner root must be absolute and canonical")
    files = []
    total = 0
    missing = []
    for name in ROOTS:
        base = root / name
        require(base == base.resolve(), "evidence root contains a symlink")
        if not base.exists():
            missing.append(name)
            continue
        paths = base.rglob("*") if base.is_dir() else [base]
        for path in paths:
            info = path.lstat()
            require(not stat.S_ISLNK(info.st_mode), "evidence symlink")
            if stat.S_ISDIR(info.st_mode):
                continue
            require(stat.S_ISREG(info.st_mode), "nonregular evidence file")
            total += info.st_size
            require(total <= MAX_EXPANDED and len(files) < MAX_FILES,
                    "evidence exceeds recovery bounds")
            relative = path.relative_to(root).as_posix()
            safe_path(relative)
            files.append({"path": relative, "bytes": info.st_size,
                          "mode": stat.S_IMODE(info.st_mode), "sha256": sha(path)})
    require(files, "no evidence to preserve")
    return sorted(files, key=lambda item: item["path"]), missing


def pack(root, output):
    require(not output.exists(), "refusing to overwrite evidence archive")
    files, missing = inventory(root)
    index = {"format": FORMAT, "roots": ROOTS, "missing_roots": missing,
             "run_id": os.environ.get("GITHUB_RUN_ID"),
             "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
             "checkout": os.environ.get("GITHUB_SHA"),
             "producer_sha256": sha(Path(__file__)),
             "uploads": {k[7:].lower(): v for k, v in os.environ.items()
                         if k.startswith("UPLOAD_")}, "files": files}
    encoded = json.dumps(index, sort_keys=True, separators=(",", ":")).encode()
    require(len(encoded) <= MAX_INDEX, "evidence index exceeds recovery bound")
    blobs = {}
    for entry in files:
        blobs.setdefault(entry["sha256"], entry)
    require(not output.resolve().is_relative_to(root / "receipts")
            and not output.resolve().is_relative_to(root / "native-evidence")
            and not output.resolve().is_relative_to(root / "consumer209")
            and not output.resolve().is_relative_to(root / "llvm-23.1.4"),
            "archive must be outside the input roots")
    with output.open("xb") as destination:
        process = subprocess.Popen(["zstd", "-q", "-10", "-T1", "-c"],
                                   stdin=subprocess.PIPE, stdout=destination)
        try:
            with tarfile.open(fileobj=process.stdin, mode="w|") as archive:
                info = tarfile.TarInfo("INDEX.json")
                info.size = len(encoded)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(encoded))
                # Neighbouring names share structural text; one object retains
                # every byte for all paths that refer to the same SHA-256.
                for digest, entry in sorted(blobs.items(), key=lambda pair:
                                             (Path(pair[1]["path"]).name, pair[0])):
                    info = tarfile.TarInfo("objects/" + digest)
                    info.size = entry["bytes"]
                    info.mode = 0o644
                    with (root / entry["path"]).open("rb") as source:
                        archive.addfile(info, source)
                    require(destination.tell() <= MAX_ARCHIVE,
                            "compressed evidence exceeds recovery bound")
        except BaseException:
            process.kill()
            raise
        finally:
            process.stdin.close()
            result = process.wait()
        require(result == 0, "zstd evidence compression failed")
    require(output.stat().st_size <= MAX_ARCHIVE,
            "compressed evidence exceeds recovery bound")
    verify(output)
    require(inventory(root) == (files, missing), "evidence changed while archiving")
    return index


class BoundedReader:
    def __init__(self, stream):
        self.stream = stream
        self.remaining = MAX_EXPANDED + MAX_INDEX + MAX_FILES * 2048

    def read(self, amount=-1):
        require(amount >= 0, "unbounded evidence read")
        data = self.stream.read(min(amount, self.remaining + 1))
        self.remaining -= len(data)
        require(self.remaining >= 0, "expanded archive exceeds recovery bound")
        return data


def verify(path, restore=None):
    require(0 < path.stat().st_size <= MAX_ARCHIVE, "archive size exceeds bound")
    if restore is not None:
        require(not restore.exists() and not restore.is_symlink(),
                "restore requires a new directory")
        restore.mkdir(parents=True)
    process = subprocess.Popen(["zstd", "-q", "-d", "--memory=64MB", "-c", str(path)],
                               stdout=subprocess.PIPE)
    try:
        reader = BoundedReader(process.stdout)
        with tarfile.open(fileobj=reader, mode="r|", ignore_zeros=True) as archive:
            members = iter(archive)
            first = next(members, None)
            require(first is not None and first.name == "INDEX.json"
                    and first.isfile() and 0 < first.size <= MAX_INDEX,
                    "missing or oversized evidence index")
            index = json.loads(archive.extractfile(first).read())
            require(index.get("format") == FORMAT and isinstance(index.get("files"), list)
                    and 0 < len(index["files"]) <= MAX_FILES, "invalid evidence index")
            references = {}
            names = set()
            total = 0
            for entry in index["files"]:
                name = entry["path"]
                safe_path(name)
                require(name not in names, "duplicate evidence path")
                names.add(name)
                size, mode, digest = entry["bytes"], entry["mode"], entry["sha256"]
                require(type(size) is int and size >= 0
                        and type(mode) is int and 0 <= mode <= 0o7777
                        and isinstance(digest, str) and re.fullmatch("[0-9a-f]{64}", digest),
                        "invalid evidence entry")
                total += size
                require(total <= MAX_EXPANDED, "logical evidence exceeds recovery bound")
                prior = references.setdefault(digest, [])
                require(not prior or prior[0]["bytes"] == size, "inconsistent blob size")
                prior.append(entry)
            seen = set()
            require(not any(parent.as_posix() in names for name in names
                            for parent in PurePosixPath(name).parents),
                    "file used as evidence directory")
            for member in members:
                require(member.isfile() and member.name.startswith("objects/"),
                        "unexpected evidence member")
                digest = member.name.removeprefix("objects/")
                require(digest in references and digest not in seen,
                        "unexpected or duplicate evidence blob")
                seen.add(digest)
                entries = references[digest]
                require(member.size == entries[0]["bytes"], "blob size mismatch")
                source = archive.extractfile(member)
                hasher = hashlib.sha256()
                primary = None
                if restore is not None:
                    primary = restore / entries[0]["path"]
                    primary.parent.mkdir(parents=True, exist_ok=True)
                target = primary.open("xb") if primary else None
                try:
                    while chunk := source.read(1024 * 1024):
                        hasher.update(chunk)
                        if target:
                            target.write(chunk)
                finally:
                    if target:
                        target.close()
                require(hasher.hexdigest() == digest, "evidence blob digest mismatch")
                if primary:
                    primary.chmod(entries[0]["mode"])
                    for entry in entries[1:]:
                        duplicate = restore / entry["path"]
                        duplicate.parent.mkdir(parents=True, exist_ok=True)
                        with primary.open("rb") as source, duplicate.open("xb") as target:
                            shutil.copyfileobj(source, target)
                        duplicate.chmod(entry["mode"])
            require(seen == references.keys(), "missing evidence blob")
        # Reject nonzero extra payload and bound decompression even past tar EOF.
        while chunk := reader.read(1024 * 1024):
            require(not any(chunk), "trailing nonzero archive data")
        require(process.wait() == 0, "zstd evidence decompression failed")
        return index
    finally:
        if process.poll() is None:
            process.kill()
        process.stdout.close()
        process.wait()


def emit(path, stream=sys.stdout):
    index = verify(path)
    size = path.stat().st_size
    header = {"sha256": sha(path), "bytes": size,
              "chunks": (size + CHUNK - 1) // CHUNK, "files": len(index["files"]),
              "run_id": index.get("run_id"), "checkout": index.get("checkout")}
    print(MARKER, "BEGIN", json.dumps(header, sort_keys=True), file=stream)
    with path.open("rb") as source:
        number = 0
        while chunk := source.read(CHUNK):
            print(MARKER, "CHUNK", number, base64.b64encode(chunk).decode(), file=stream)
            number += 1
    print(MARKER, "END", header["sha256"], file=stream, flush=True)
    return header


def recover(log, output):
    require(not output.exists(), "refusing to overwrite recovered archive")
    header = None
    number = total = 0
    ended = False
    digest = hashlib.sha256()
    pattern = re.compile(r"(?:^|\s)" + MARKER + r" (BEGIN|CHUNK|END) (.*)$")
    with output.open("xb") as target, log.open(errors="strict") as source:
        for line in source:
            match = pattern.search(line.rstrip("\r\n"))
            if not match:
                continue
            kind, value = match.groups()
            require(not ended, "evidence records after END")
            if kind == "BEGIN":
                require(header is None, "duplicate evidence BEGIN")
                header = json.loads(value)
                require(type(header.get("bytes")) is int and 0 < header["bytes"] <= MAX_ARCHIVE
                        and header.get("chunks") == (header["bytes"] + CHUNK - 1) // CHUNK
                        and isinstance(header.get("sha256"), str)
                        and re.fullmatch("[0-9a-f]{64}", header["sha256"]),
                        "invalid evidence transport header")
            elif kind == "CHUNK":
                require(header is not None, "evidence chunk before BEGIN")
                ordinal, payload = value.split(" ", 1)
                require(ordinal == str(number) and number < header["chunks"],
                        "missing, duplicate or reordered evidence chunk")
                data = base64.b64decode(payload, validate=True)
                require(len(data) == min(CHUNK, header["bytes"] - total),
                        "wrong evidence chunk length")
                target.write(data)
                digest.update(data)
                total += len(data)
                number += 1
            else:
                require(header is not None and value == header["sha256"]
                        and number == header["chunks"] and total == header["bytes"]
                        and digest.hexdigest() == header["sha256"],
                        "incomplete or corrupt evidence log")
                ended = True
    require(ended, "missing complete evidence transport")
    index = verify(output)
    require(len(index["files"]) == header.get("files")
            and index.get("run_id") == header.get("run_id")
            and index.get("checkout") == header.get("checkout"),
            "transport identity differs from evidence index")
    return header


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    pack_parser = sub.add_parser("pack")
    pack_parser.add_argument("root", type=Path)
    pack_parser.add_argument("archive", type=Path)
    for action in ("emit", "verify"):
        child = sub.add_parser(action)
        child.add_argument("archive", type=Path)
        if action == "verify":
            child.add_argument("--restore", type=Path)
    recovery = sub.add_parser("recover")
    recovery.add_argument("log", type=Path)
    recovery.add_argument("archive", type=Path)
    args = parser.parse_args()
    if args.action == "pack":
        index = pack(args.root, args.archive)
        print(json.dumps({"archive": str(args.archive), "sha256": sha(args.archive),
                          "bytes": args.archive.stat().st_size, "files": len(index["files"])}))
    elif args.action == "emit":
        emit(args.archive)
    elif args.action == "verify":
        index = verify(args.archive, args.restore)
        print(json.dumps({"sha256": sha(args.archive), "files": len(index["files"]),
                          "run_id": index.get("run_id"), "checkout": index.get("checkout")}))
    else:
        print(json.dumps(recover(args.log, args.archive), sort_keys=True))


if __name__ == "__main__":
    main()

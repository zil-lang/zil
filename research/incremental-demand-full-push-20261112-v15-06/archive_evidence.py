#!/usr/bin/env python3
"""Preserve explicitly selected raw evidence; no network, Git, or source writes.

pack --source-root ROOT --archive-dir NEW_DIR --include 'out/runs/name/**'
     --source-commit candidate=FULL_SHA --main-commit FULL_SHA --scope TEXT
verify --archive-dir DIR --restore-dir NEW_DIR

Outputs are evidence-manifest.json and ordered 24 MiB compressed parts. Tar
headers are normalized; regular file bytes and permission bits are preserved.
Commit IDs are caller-declared provenance, not an assertion about a Git tree.
"""
import argparse
import fnmatch
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

PART_BYTES = 24 * 1024 * 1024
CHUNK = 1024 * 1024
MAX_METADATA = 64 * 1024 * 1024
MANIFEST = "evidence-manifest.json"
INVENTORY = ".evidence-metadata/inventory.json"
PAYLOAD = "payload/"
SECRET_DIRS = {".aws", ".ssh", ".gnupg", ".kube", ".azure", ".codex"}
SECRET_FILES = {"auth.json", "credentials", "credentials.json", "id_rsa", "id_ed25519", "secrets.json"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode()


def relative(value, evidence=False):
    require(isinstance(value, str) and value and "\\" not in value and "\0" not in value,
            "invalid relative path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and all(p not in ("", ".", "..") for p in value.split("/")),
            "absolute, empty, or parent path rejected: " + value)
    value.encode("utf-8")
    if evidence:
        require(path.parts[0] != ".evidence-metadata", "reserved metadata path")
        require(not any(p in SECRET_DIRS or p in SECRET_FILES or p == ".env" or p.startswith(".env.")
                        for p in path.parts), "sensitive path rejected: " + value)
    return path


def real_path(value):
    path = Path(os.path.abspath(value))
    for component in [*reversed(path.parents), path]:
        try:
            require(not stat.S_ISLNK(component.lstat().st_mode), "symlink path rejected: " + str(component))
        except FileNotFoundError:
            pass
    return path


class Root:
    """All traversals and opens stay below a held directory, without symlinks."""
    def __init__(self, path):
        self.path = real_path(path)
        self.fd = os.open(self.path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)

    def close(self):
        os.close(self.fd)

    def directory(self, parts, create=False):
        fd = os.dup(self.fd)
        try:
            for part in parts:
                if create:
                    try:
                        os.mkdir(part, 0o700, dir_fd=fd)
                    except FileExistsError:
                        pass
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = child
            return fd
        except BaseException:
            os.close(fd)
            raise

    def entries(self, parts):
        fd = self.directory(parts)
        try:
            with os.scandir(fd) as entries:
                return sorted((e.name, e.stat(follow_symlinks=False)) for e in entries)
        finally:
            os.close(fd)

    def file(self, name, create=False):
        parts = relative(name).parts
        parent = self.directory(parts[:-1], create=create)
        try:
            flags = (os.O_WRONLY | os.O_CREAT | os.O_EXCL) if create else os.O_RDONLY
            fd = os.open(parts[-1], flags | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        finally:
            os.close(parent)
        try:
            require(stat.S_ISREG(os.fstat(fd).st_mode), "nonregular file rejected: " + name)
            return os.fdopen(fd, "wb" if create else "rb")
        except BaseException:
            os.close(fd)
            raise


def selected_files(root, selectors, archive):
    selected = set()
    matched = 0

    def collect(parts, metadata):
        nonlocal matched
        name = "/".join(parts)
        relative(name, evidence=True)
        if stat.S_ISDIR(metadata.st_mode):
            require(not archive.is_relative_to(root.path.joinpath(*parts)), "archive directory is inside selected evidence")
            for child, info in root.entries(parts):
                collect((*parts, child), info)
        else:
            require(stat.S_ISREG(metadata.st_mode), "symlink/special file rejected: " + name)
            matched += 1
            selected.add(name)

    def expand(parts, pattern):
        if pattern[0] == "**":
            if len(pattern) == 1:
                require(parts, "select an explicit evidence directory, not the whole source root")
                fd = root.directory(parts)
                try:
                    collect(parts, os.fstat(fd))
                finally:
                    os.close(fd)
            else:
                expand(parts, pattern[1:])
                for name, info in root.entries(parts):
                    require(not stat.S_ISLNK(info.st_mode), "symlink rejected under recursive selector")
                    if stat.S_ISDIR(info.st_mode):
                        relative("/".join((*parts, name)), evidence=True)
                        expand((*parts, name), pattern)
            return
        for name, info in root.entries(parts):
            if not fnmatch.fnmatchcase(name, pattern[0]):
                continue
            child = (*parts, name)
            relative("/".join(child), evidence=True)
            require(not stat.S_ISLNK(info.st_mode), "symlink rejected: " + "/".join(child))
            if len(pattern) == 1:
                collect(child, info)
            elif stat.S_ISDIR(info.st_mode):
                expand(child, pattern[1:])
            elif not stat.S_ISREG(info.st_mode):
                raise ValueError("special file rejected: " + "/".join(child))

    for selector in selectors:
        before = matched
        expand((), relative(selector, evidence=True).parts)
        require(matched > before, "selector matched no regular evidence: " + selector)
    require(selected, "no regular evidence selected")
    return sorted(selected)


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_mode)


def hash_file(file):
    digest, size = hashlib.sha256(), 0
    while data := file.read(CHUNK):
        digest.update(data)
        size += len(data)
    return digest.hexdigest(), size


def tar_info(name, size, mode):
    info = tarfile.TarInfo(name)
    info.size, info.mode, info.mtime = size, mode, 0
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    return info


def tar_size(inventory, metadata):
    records = [(INVENTORY, len(metadata), 0o600)] + [(PAYLOAD + f["path"], f["bytes"], f["mode"]) for f in inventory["files"]]
    size = sum(len(tar_info(name, length, mode).tobuf(tarfile.PAX_FORMAT)) + ((length + 511) // 512) * 512
               for name, length, mode in records) + 1024
    return ((size + tarfile.RECORDSIZE - 1) // tarfile.RECORDSIZE) * tarfile.RECORDSIZE


class HashedReader:
    def __init__(self, file, maximum=None):
        self.file, self.maximum, self.size, self.digest = file, maximum, 0, hashlib.sha256()

    def read(self, size=-1):
        if self.maximum is not None:
            size = min(size if size >= 0 else CHUNK, self.maximum - self.size + 1)
        data = self.file.read(size)
        self.size += len(data)
        require(self.maximum is None or self.size <= self.maximum, "stream exceeds declared bound")
        self.digest.update(data)
        return data


def pack(args):
    source, archive = Root(args.source_root), real_path(args.archive_dir)
    created = []
    try:
        require(archive != source.path, "archive directory cannot be the source root")
        require(archive.parent.is_dir(), "archive parent must already exist")
        require(not archive.exists() or archive.is_dir() and not any(archive.iterdir()), "archive directory must be new or empty")
        names = selected_files(source, args.include, archive)
        files, stamps = [], {}
        for name in names:
            with source.file(name) as file:
                before = os.fstat(file.fileno())
                digest, size = hash_file(file)
                require(stamp(before) == stamp(os.fstat(file.fileno())) and size == before.st_size, "source changed while hashing: " + name)
                files.append({"path": name, "bytes": size, "sha256": digest, "mode": stat.S_IMODE(before.st_mode) & 0o777})
                stamps[name] = stamp(before)
        commits = {}
        for value in args.source_commit:
            label, separator, commit = value.partition("=")
            require(separator and re.fullmatch(r"[A-Za-z][A-Za-z0-9_.-]{0,63}", label) and label not in commits, "source commit must be unique LABEL=SHA")
            require(re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", commit), "invalid source commit")
            commits[label] = commit.lower()
        require(re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", args.main_commit), "invalid main commit")
        inventory = {"schema": 1, "source_commits": commits, "main_commit": args.main_commit.lower(), "scope": args.scope, "files": files}
        metadata = canonical(inventory)
        require(len(metadata) <= MAX_METADATA, "inventory metadata exceeds 64 MiB")
        archive.mkdir(mode=0o700, exist_ok=True)
        with tempfile.TemporaryFile(dir=archive, prefix=".evidence-build-") as compressed, tempfile.TemporaryFile() as errors:
            process = subprocess.Popen(["zstd", "-q", "-T1", f"-{args.level}", "-c"], stdin=subprocess.PIPE, stdout=compressed, stderr=errors)
            try:
                with tarfile.open(fileobj=process.stdin, mode="w|", format=tarfile.PAX_FORMAT) as tar:
                    tar.addfile(tar_info(INVENTORY, len(metadata), 0o600), io.BytesIO(metadata))
                    for entry in files:
                        with source.file(entry["path"]) as file:
                            require(stamp(os.fstat(file.fileno())) == stamps[entry["path"]], "source changed before archiving: " + entry["path"])
                            reader = HashedReader(file)
                            tar.addfile(tar_info(PAYLOAD + entry["path"], entry["bytes"], entry["mode"]), reader)
                            require(reader.digest.hexdigest() == entry["sha256"] and reader.size == entry["bytes"]
                                    and stamp(os.fstat(file.fileno())) == stamps[entry["path"]], "source changed during archiving: " + entry["path"])
                process.stdin.close()
                require(process.wait() == 0, "zstd compression failed")
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait()
            compressed.seek(0)
            parts, whole, total = [], hashlib.sha256(), 0
            while data := compressed.read(PART_BYTES):
                name = f"evidence.tar.zst.part-{len(parts) + 1:05d}"
                destination = archive / name
                with destination.open("xb") as file:
                    created.append(destination)
                    file.write(data)
                    file.flush()
                    os.fsync(file.fileno())
                parts.append({"name": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
                whole.update(data)
                total += len(data)
        manifest = {"schema": 1, "source_root": str(source.path), "selectors": args.include,
                    "archive": {"format": "tar+zstd", "bytes": total, "sha256": whole.hexdigest(),
                                "tar_bytes": tar_size(inventory, metadata), "part_bytes": PART_BYTES, "parts": parts},
                    "inventory_sha256": hashlib.sha256(metadata).hexdigest(), "inventory": inventory}
        with (archive / MANIFEST).open("xb") as file:
            created.append(archive / MANIFEST)
            file.write(canonical(manifest))
            file.flush()
            os.fsync(file.fileno())
        return {"status": "packed", "manifest": str(archive / MANIFEST), "files": len(files),
                "raw_bytes": sum(f["bytes"] for f in files), "archive_bytes": total, "parts": len(parts), "sha256": whole.hexdigest()}
    except BaseException:
        for path in reversed(created):
            path.unlink(missing_ok=True)
        raise
    finally:
        source.close()


def verify(args):
    archive = Root(args.archive_dir)
    destination = real_path(args.restore_dir)
    require(not destination.exists() and not destination.is_symlink(), "restore directory must not exist")
    try:
        with archive.file(MANIFEST) as file:
            data = file.read(MAX_METADATA + 1)
        require(len(data) <= MAX_METADATA, "manifest exceeds 64 MiB")
        manifest = json.loads(data)
        require(manifest["schema"] == 1 and manifest["inventory"]["schema"] == 1, "unsupported manifest schema")
        inventory, description = manifest["inventory"], manifest["archive"]
        metadata = canonical(inventory)
        require(hashlib.sha256(metadata).hexdigest() == manifest["inventory_sha256"], "inventory digest mismatch")
        entries = {}
        for entry in inventory["files"]:
            name = str(relative(entry["path"], evidence=True))
            require(name not in entries and type(entry["bytes"]) is int and entry["bytes"] >= 0
                    and type(entry["mode"]) is int and 0 <= entry["mode"] <= 0o777
                    and re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]), "invalid inventory entry")
            entries[name] = entry
        require(entries and list(entries) == sorted(entries), "inventory must be nonempty and sorted")
        require(all(str(parent) not in entries for name in entries for parent in PurePosixPath(name).parents), "file/directory inventory collision")
        expected_tar = tar_size(inventory, metadata)
        require(description["format"] == "tar+zstd" and description["part_bytes"] == PART_BYTES
                and description["tar_bytes"] == expected_tar and description["parts"], "invalid archive descriptor")
        with tempfile.TemporaryFile(prefix="evidence-recovery-") as compressed:
            whole, total = hashlib.sha256(), 0
            for index, part in enumerate(description["parts"], 1):
                require(part["name"] == f"evidence.tar.zst.part-{index:05d}" and type(part["bytes"]) is int
                        and 0 < part["bytes"] <= PART_BYTES, "invalid part descriptor")
                if index != len(description["parts"]):
                    require(part["bytes"] == PART_BYTES, "short nonfinal part")
                with archive.file(part["name"]) as file:
                    data = file.read(PART_BYTES + 1)
                require(len(data) == part["bytes"] and hashlib.sha256(data).hexdigest() == part["sha256"], "part size/digest mismatch")
                whole.update(data)
                total += len(data)
                compressed.write(data)
            require(total == description["bytes"] and whole.hexdigest() == description["sha256"], "complete archive digest mismatch")
            require(destination.parent.is_dir(), "restore parent must already exist")
            destination.mkdir(mode=0o700)
            restored = Root(destination)
            compressed.seek(0)
            seen = set()
            with tempfile.TemporaryFile() as errors:
                process = subprocess.Popen(["zstd", "-q", "-d", "-c"], stdin=compressed, stdout=subprocess.PIPE, stderr=errors)
                reader = HashedReader(process.stdout, maximum=expected_tar)
                try:
                    with tarfile.open(fileobj=reader, mode="r|", ignore_zeros=True) as tar:
                        for member in tar:
                            require(member.type == tarfile.REGTYPE and not member.linkname and not member.issparse(), "nonregular tar member rejected")
                            require(member.name not in seen, "duplicate tar member")
                            if member.name == INVENTORY:
                                require(not seen and member.size == len(metadata), "misplaced inventory")
                                require(tar.extractfile(member).read(len(metadata) + 1) == metadata, "internal inventory mismatch")
                                with restored.file(INVENTORY, create=True) as file:
                                    file.write(metadata)
                            else:
                                require(INVENTORY in seen and member.name.startswith(PAYLOAD), "unknown tar member")
                                name = str(relative(member.name[len(PAYLOAD):], evidence=True))
                                require(name in entries and member.size == entries[name]["bytes"], "unlisted member or length mismatch")
                                with restored.file(name, create=True) as file:
                                    content = tar.extractfile(member)
                                    digest, length = hashlib.sha256(), 0
                                    while data := content.read(CHUNK):
                                        file.write(data)
                                        digest.update(data)
                                        length += len(data)
                                    require(length == entries[name]["bytes"] and digest.hexdigest() == entries[name]["sha256"], "restored stream digest mismatch")
                                    os.fchmod(file.fileno(), entries[name]["mode"])
                            seen.add(member.name)
                    while reader.read(CHUNK):
                        pass
                    require(reader.size == expected_tar and process.wait() == 0, "decompression length or zstd integrity failure")
                finally:
                    if process.poll() is None:
                        process.terminate()
                        process.wait()
                    process.stdout.close()
                    restored.close()
        actual = selected_files_for_verification(destination)
        require(actual == set(entries) | {INVENTORY}, "extra or missing restored files")
        root = Root(destination)
        try:
            for name, entry in entries.items():
                with root.file(name) as file:
                    digest, length = hash_file(file)
                    require((digest, length) == (entry["sha256"], entry["bytes"]), "post-restore file digest mismatch")
            with root.file(INVENTORY) as file:
                require(file.read(MAX_METADATA + 1) == metadata, "restored inventory mismatch")
        finally:
            root.close()
        return {"status": "verified", "restore_dir": str(destination), "files": len(entries),
                "raw_bytes": sum(f["bytes"] for f in entries.values()), "archive_bytes": total,
                "parts": len(description["parts"]), "sha256": whole.hexdigest(),
                "source_commits": inventory["source_commits"], "main_commit": inventory["main_commit"], "scope": inventory["scope"]}
    finally:
        archive.close()


def selected_files_for_verification(path):
    root, result = Root(path), set()
    def walk(parts):
        for name, info in root.entries(parts):
            child = (*parts, name)
            if stat.S_ISDIR(info.st_mode):
                walk(child)
            else:
                require(stat.S_ISREG(info.st_mode), "nonregular restored file")
                result.add("/".join(child))
    try:
        walk(())
        return result
    finally:
        root.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("pack")
    create.add_argument("--source-root", required=True)
    create.add_argument("--archive-dir", required=True)
    create.add_argument("--include", action="append", required=True)
    create.add_argument("--source-commit", action="append", required=True, metavar="LABEL=SHA")
    create.add_argument("--main-commit", required=True)
    create.add_argument("--scope", required=True)
    create.add_argument("--level", type=int, choices=range(1, 20), default=6)
    check = commands.add_parser("verify")
    check.add_argument("--archive-dir", required=True)
    check.add_argument("--restore-dir", required=True)
    args = parser.parse_args()
    try:
        receipt = pack(args) if args.command == "pack" else verify(args)
        print(json.dumps(receipt, sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError, tarfile.TarError) as error:
        print(json.dumps({"status": "error", "error": str(error)}, sort_keys=True), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

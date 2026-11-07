"""Independently compare a tool-evidence tar.xz with its two frozen input trees.

Read-only: does not extract members, follow fixture symlinks or execute tools.
The command prints a JSON receipt; the caller records it outside the inputs.
"""
import argparse
import hashlib
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import stat
import tarfile


CHUNK = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(CHUNK), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_name(name):
    path = PurePosixPath(name)
    require(bool(path.parts) and not path.is_absolute() and path.as_posix() == name
            and not any(part in (".", "..") for part in path.parts)
            and not any(char in name for char in ("\\", ":", "\0")), "unsafe path: " + name)
    return path


def inventory(root):
    entries, pending = {}, [(root, ".")]
    while pending:
        path, name = pending.pop()
        info = path.lstat()
        entries[name] = (info.st_mode, info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)
        if stat.S_ISDIR(info.st_mode):
            with os.scandir(path) as children:
                for child in children:
                    relative = child.name if name == "." else name + "/" + child.name
                    safe_name(relative)
                    pending.append((Path(child.path), relative))
    return entries


def audit(archive_path, expected_sha, roots):
    original_sha = sha_file(archive_path)
    require(original_sha == expected_sha, "outer archive SHA differs")
    before = {label: inventory(path) for label, path in roots.items()}
    with tarfile.open(archive_path, "r:xz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        require(len(names) == len(set(names)), "duplicate tar member")
        for member in members:
            safe_name(member.name)
            require(member.isfile(), "active special tar member")
        names_set = set(names)
        require(not any(parent.as_posix() in names_set
                        for name in names for parent in safe_name(name).parents),
                "file also serves as a directory")
        manifest = json.load(archive.extractfile("MANIFEST.json"))
        index = json.load(archive.extractfile("INDEX.json"))
        require(index["format"] == "zil-delivery-tool-evidence-v1", "wrong index format")
        require(index["sources"] == {label: str(root) for label, root in roots.items()},
                "source roots differ from explicit review inputs")
        require(set(manifest) == names_set - {"MANIFEST.json"}, "manifest member set differs")
        for member in members:
            digest, count = hashlib.sha256(), 0
            with archive.extractfile(member) as stream:
                for chunk in iter(lambda: stream.read(CHUNK), b""):
                    count += len(chunk)
                    digest.update(chunk)
            require(count == member.size, "tar member length differs")
            if member.name != "MANIFEST.json":
                require(manifest[member.name] == {"bytes": count, "sha256": digest.hexdigest()},
                        "manifest digest differs: " + member.name)
        observed, stored, excluded, links, directories = set(), [], [], [], []
        for entry in index["entries"]:
            label, name = entry["source"], entry["path"]
            require(label in roots, "unknown source label")
            require((label, name) not in observed, "duplicate source path")
            observed.add((label, name))
            if name != ".":
                safe_name(name)
            path = roots[label] if name == "." else roots[label].joinpath(*PurePosixPath(name).parts)
            info = path.lstat()
            require(oct(stat.S_IMODE(info.st_mode)) == entry["mode"], "original mode changed")
            if entry["kind"] == "directory":
                require(stat.S_ISDIR(info.st_mode) and entry["disposition"] == "metadata-only",
                        "invalid directory metadata")
                directories.append(entry)
            elif entry["kind"] == "symlink":
                require(stat.S_ISLNK(info.st_mode) and entry["disposition"] == "metadata-only",
                        "invalid symlink metadata")
                require(os.readlink(path) == entry["target"], "symlink target changed")
                require("stored_path" not in entry, "symlink became an active member")
                links.append(entry)
            else:
                require(entry["kind"] == "file" and stat.S_ISREG(info.st_mode), "nonregular file")
                require(info.st_size == entry["bytes"] and sha_file(path) == entry["sha256"],
                        "original file identity changed: " + name)
                if entry["disposition"] == "stored":
                    expected = "sources/" + label + "/" + name
                    require(entry["stored_path"] == expected and expected in manifest,
                            "wrong original-to-member mapping")
                    require(manifest[expected] == {"bytes": entry["bytes"], "sha256": entry["sha256"]},
                            "stored payload identity differs")
                    with path.open("rb") as left, archive.extractfile(expected) as right:
                        while True:
                            a, b = left.read(CHUNK), right.read(CHUNK)
                            require(a == b, "original bytes differ: " + name)
                            if not a:
                                break
                    stored.append(entry)
                else:
                    require(entry["disposition"] == "hash-only-execution-product"
                            and path.name == "native.exe", "unexpected exclusion")
                    require("stored_path" not in entry, "excluded native became a member")
                    excluded.append(entry)
        require(observed == {(label, name) for label, tree in before.items() for name in tree},
                "original tree entry is missing or spurious")
        require(names_set == {entry["stored_path"] for entry in stored}
                | {"INDEX.json", "MANIFEST.json", "README.md", "packaging/pack_tool_evidence.py"},
                "unmapped archive member")
        stats = {"source_entries": len(observed), "stored_regular_files": len(stored),
                 "stored_original_bytes": sum(entry["bytes"] for entry in stored),
                 "hash_only_files": len(excluded),
                 "hash_only_bytes": sum(entry["bytes"] for entry in excluded),
                 "metadata_only_special_entries": len(links), "directories_recorded": len(directories)}
        require(stats == index["statistics"], "statistics differ")
    # Complete decoding also checks the XZ footer after tar's end marker.
    with lzma.open(archive_path, "rb") as stream:
        for _ in iter(lambda: stream.read(CHUNK), b""):
            pass
    require({label: inventory(path) for label, path in roots.items()} == before, "source tree changed")
    require(sha_file(archive_path) == original_sha, "archive changed during review")
    return {"reviewer": "/root/impl197", "verdict": "Pass", "archive": str(archive_path),
            "bytes": archive_path.stat().st_size, "sha256": original_sha,
            "regular_members": len(members), "manifest_bound_members": len(manifest),
            "statistics": stats, "original_trees_unchanged": True,
            "stored_original_files_compared_byte_for_byte": len(stored),
            "extraction": False, "links_followed_or_activated": False,
            "compiler_or_tool_tests_executed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("sha256")
    parser.add_argument("author_root", type=Path)
    parser.add_argument("reviewer_root", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.archive, args.sha256,
                           {"author": args.author_root, "reviewer": args.reviewer_root}), indent=2))

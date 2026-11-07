"""Independently stream-compare a compact evidence ZIP and its solid tar.xz.

No extraction, imports from the archive, compiler execution or source writes.
All member bytes, manifest mappings and logical index references are checked.
"""
import argparse
import hashlib
import json
import lzma
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
import zipfile


CHUNK = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def file_sha(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(CHUNK), b""):
            value.update(chunk)
    return value.hexdigest()


def identity(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode), "input is not a real regular file")
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns)


def safe_path(name):
    path = PurePosixPath(name)
    require(bool(path.parts) and not path.is_absolute() and path.as_posix() == name
            and not any(part in (".", "..") for part in path.parts)
            and not any(char in name for char in ("\\", ":", "\0")), "unsafe path: " + name)
    return path


def label(name):
    require(isinstance(name, str) and re.fullmatch(r"[a-z0-9][a-z0-9_-]*", name), "invalid label")
    return name


def audit(source, target, source_sha, target_sha):
    before = [identity(path) for path in (source, target)]
    require(file_sha(source) == source_sha and file_sha(target) == target_sha, "input SHA differs")
    require(target.stat().st_size < 50_000_000, "final container is not under 50 MB")
    with zipfile.ZipFile(source) as original:
        entries = original.infolist()
        names = [entry.filename for entry in entries]
        name_set = set(names)
        require(len(names) == len(name_set), "duplicate ZIP member")
        for entry in entries:
            safe_path(entry.filename)
            require(not entry.is_dir() and stat.S_IFMT(entry.external_attr >> 16) in (0, stat.S_IFREG),
                    "nonregular ZIP member")
            require(not any(parent.as_posix() in name_set for parent in safe_path(entry.filename).parents),
                    "file serves as a parent directory")
        manifest = json.loads(original.read("MANIFEST.json"))
        require(set(manifest) == name_set - {"MANIFEST.json"}, "manifest membership differs")
        observed, total, count = {}, 0, 0
        with lzma.open(target, "rb") as decoded:
            with tarfile.open(fileobj=decoded, mode="r|") as archive:
                for member in archive:
                    require(count < len(entries) and member.isfile(), "extra or special tar member")
                    expected = entries[count]
                    require(member.name == expected.filename and member.size == expected.file_size,
                            "ZIP/tar order, path or size differs")
                    digest, size = hashlib.sha256(), 0
                    with original.open(expected) as left, archive.extractfile(member) as right:
                        while True:
                            a, b = left.read(CHUNK), right.read(CHUNK)
                            require(a == b, "member bytes differ: " + member.name)
                            if not a:
                                break
                            size += len(b)
                            digest.update(b)
                    require(size == member.size, "decoded member length differs")
                    observed[member.name] = {"bytes": size, "sha256": digest.hexdigest()}
                    if member.name != "MANIFEST.json":
                        require(observed[member.name] == manifest[member.name], "manifest SHA differs")
                    count += 1
                    total += size
            # Consume the complete XZ stream, including its integrity footer.
            for _ in iter(lambda: decoded.read(CHUNK), b""):
                pass
        require(count == len(entries), "missing tar member")
        index = json.loads(original.read("INDEX.json"))
        require(index["format"] == "zil-evidence-sha256-v1", "unexpected compact format")
        sources = {}
        for item in index["sources"]:
            key = label(item["id"])
            label(item["component"])
            require(key not in sources, "duplicate source identity")
            sources[key] = item
        targets, objects = set(), set()
        for item in index["files"]:
            require(item["source"] in sources, "unknown source")
            require(item["component"] == sources[item["source"]]["component"], "component mismatch")
            safe_path(item["path"])
            expected = "components/" + item["component"] + "/" + item["source"] + "/" + item["path"]
            require(item["restore_path"] == expected and expected not in targets, "bad restore mapping")
            targets.add(expected)
            sha = item["sha256"]
            require(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{64}", sha), "bad object digest")
            stored = "records/sha256/" + sha[:2] + "/" + sha
            require(item["stored_path"] == stored and manifest.get(stored)
                    == {"bytes": item["bytes"], "sha256": sha}, "bad object reference")
            objects.add(stored)
        require(objects == {name for name in names if name.startswith("records/")},
                "missing or unreferenced payload object")
        for item in index["excluded_execution_products"]:
            require(item["source"] in sources
                    and item["component"] == sources[item["source"]]["component"], "bad exclusion source")
            safe_path(item["path"])
            require(isinstance(item["bytes"], int) and item["bytes"] >= 0
                    and re.fullmatch(r"[0-9a-f]{64}", item["sha256"]), "bad exclusion identity")
        statistics = {"logical_files": len(index["files"]),
                      "logical_bytes": sum(item["bytes"] for item in index["files"]),
                      "unique_contents": len(objects),
                      "unique_bytes": sum(manifest[name]["bytes"] for name in objects),
                      "excluded_execution_products": len(index["excluded_execution_products"])}
        require(index["statistics"] == statistics, "index statistics differ")
    require([identity(path) for path in (source, target)] == before, "input metadata changed")
    require(file_sha(source) == source_sha and file_sha(target) == target_sha, "input bytes changed")
    return {"reviewer": "/root/impl197", "verdict": "Pass",
            "source_zip": {"path": str(source), "bytes": before[0][3], "sha256": source_sha},
            "solid_archive": {"path": str(target), "bytes": before[1][3], "sha256": target_sha},
            "members_compared_byte_for_byte": count, "decoded_member_bytes": total,
            "manifest_bound_members": len(manifest), "manifest_identity": observed["MANIFEST.json"],
            "index_identity": observed["INDEX.json"], "statistics": statistics,
            "source_count": len(sources), "input_bytes_and_metadata_unchanged": True,
            "zip_member_crc_and_xz_footer_checked": True, "extraction": False,
            "compiler_or_packaged_tools_executed": False,
            "scope": "Exact frozen ZIP member preservation plus self-contained manifest/index integrity; historical compiler executions are not repeated or relabeled."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_zip", type=Path)
    parser.add_argument("tar_xz", type=Path)
    parser.add_argument("source_sha256")
    parser.add_argument("target_sha256")
    args = parser.parse_args()
    print(json.dumps(audit(args.source_zip, args.tar_xz, args.source_sha256, args.target_sha256), indent=2))

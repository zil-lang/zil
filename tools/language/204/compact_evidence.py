#!/usr/bin/env python3
"""Content-deduplicate evidence without rewriting its original paths or results."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import zipfile


FORMAT = "zil-evidence-sha256-v1"
MACHINE_SUFFIXES = {".exe", ".o", ".obj", ".a", ".lib", ".so", ".dll",
                    ".dylib", ".rlib", ".rmeta", ".pdb", ".bc", ".wasm"}
MACHINE_MAGIC = (b"\x7fELF", b"MZ", b"!<arch>\n", b"BC\xc0\xde", b"\0asm",
                 b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe",
                 b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe")
README = """# ZIL evidence: exact content deduplication

INDEX.json retains every included original component/source/path, byte count,
and SHA256. Identical contents share one records/sha256/ object across all
components. Original archive manifests and their exclusion lists remain exact
payloads. This preserves file bytes and paths, not the original ZIP container
encoding. Original ZIP identities remain in INDEX.json; no original is changed.

This is an evidence container, not a new compiler test or acceptance result.
Read each original receipt for its actual source/CLI identity, Fail/NotRun/Pass,
and author/reviewer distinction. Earlier failures and CLI stages stay separate.

Native executables, objects and build products are excluded with hashes for
directory inputs. Binary Wire test inputs (including malformed/truncated
inputs), CT program Wire, cache candidates and exact binary standard I/O
remain test evidence and are not execution products. Sources, IR, logs and
empty files are preserved. No compiler, test or packaged program is executed.

Extract compact_evidence.py from this ZIP, then use Python 3 (Git Bash or
PowerShell both work):

    python compact_evidence.py verify PATH_TO_THIS_ZIP
    python compact_evidence.py restore PATH_TO_THIS_ZIP NEW_DIRECTORY
    python compact_evidence.py restore PATH_TO_THIS_ZIP NEW_DIRECTORY --component root

Verification checks all ZIP member CRCs, SHA256, sizes, object references and
original-path mappings without restoring duplicates. Restore first verifies the
whole archive, refuses an existing destination, and writes files exclusively.
Select a component to avoid materializing all duplicate evidence. INDEX.json
reports logical restored bytes and unique stored bytes. Empty directories,
timestamps and executable permission bits are not evidence payloads.
"""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def safe_path(name):
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or path.as_posix() != name
            or any(part in ("", ".", "..") for part in path.parts)
            or "\\" in name or ":" in name or "\0" in name):
        raise ValueError("unsafe evidence path: " + repr(name))
    return path


def safe_label(name):
    if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", name):
        raise ValueError("invalid component/source label: " + repr(name))
    return name


def fingerprint(path):
    info = path.stat()
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)


def file_digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def evidence_kind(name, data):
    suffix = PurePosixPath(name).suffix.lower()
    # Wire is a semantic test input, including intentionally malformed inputs.
    if suffix == ".wire" or data.startswith(b"ZILMODP\0"):
        return "binary-wire-test-evidence"
    # These are recorded test inputs/results, including invalid UTF-8 and NUL.
    # CT wire and database candidates have explicit repository format tags;
    # arbitrary .bin files remain unclassified rather than silently included.
    if data.startswith((b"ZILCTPG\0", b"ZILCTIR\0")):
        return "binary-ct-wire-test-evidence"
    if suffix == ".candidate" and data.startswith(b"ZILRAW02"):
        return "binary-cache-test-evidence"
    if suffix in (".stdin", ".stdout", ".stderr"):
        return "binary-standard-io-evidence"
    if suffix in MACHINE_SUFFIXES or data.startswith(MACHINE_MAGIC):
        return "excluded-execution-product"
    if suffix == ".bin" and not data:
        return "empty-binary-test-evidence"
    if suffix in (".zil", ".c", ".rs"):
        return "source-evidence"
    data.decode("utf-8")
    if b"\0" in data:
        raise ValueError("unclassified nontext evidence: " + name)
    return "text-evidence"


def tree_inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("evidence symlink is not followed: " + str(path))
        if path.is_file():
            name = path.relative_to(root).as_posix()
            safe_path(name)
            result[name] = fingerprint(path)
    return result


def restored_path(component, source, original):
    safe_label(component)
    safe_label(source)
    safe_path(original)
    return f"components/{component}/{source}/{original}"


def verify(archive_path):
    with zipfile.ZipFile(archive_path) as archive:
        names = [entry.filename for entry in archive.infolist()]
        if len(names) != len(set(names)):
            raise ValueError("duplicate ZIP member")
        for name in names:
            safe_path(name)
        manifest = json.loads(archive.read("MANIFEST.json"))
        if set(manifest) != set(names) - {"MANIFEST.json"}:
            raise ValueError("ZIP membership differs from manifest")
        for name, expected in manifest.items():
            data = archive.read(name)  # ZipFile verifies this member's CRC.
            if len(data) != expected["bytes"] or digest(data) != expected["sha256"]:
                raise ValueError("content mismatch: " + name)
        index = json.loads(archive.read("INDEX.json"))
        if index["format"] != FORMAT:
            raise ValueError("unknown evidence format")
        source_ids = set()
        sources = {}
        for source in index["sources"]:
            key = safe_label(source["id"])
            safe_label(source["component"])
            if key in source_ids:
                raise ValueError("duplicate source identity")
            source_ids.add(key)
            sources[key] = source
        targets, objects = set(), set()
        for entry in index["files"]:
            source = sources[entry["source"]]
            if entry["component"] != source["component"]:
                raise ValueError("source/component mismatch")
            target = restored_path(entry["component"], entry["source"], entry["path"])
            if entry["restore_path"] != target or target in targets:
                raise ValueError("invalid or duplicate restored path")
            targets.add(target)
            sha = entry["sha256"]
            if not re.fullmatch(r"[0-9a-f]{64}", sha):
                raise ValueError("invalid content digest")
            stored = f"records/sha256/{sha[:2]}/{sha}"
            if (entry["stored_path"] != stored or manifest.get(stored)
                    != {"bytes": entry["bytes"], "sha256": sha}):
                raise ValueError("invalid content object reference")
            objects.add(stored)
        if objects != {name for name in names if name.startswith("records/")}:
            raise ValueError("missing or unreferenced content object")
        for entry in index["excluded_execution_products"]:
            source = sources[entry["source"]]
            if entry["component"] != source["component"]:
                raise ValueError("excluded source/component mismatch")
            safe_path(entry["path"])
        stats = {"logical_files": len(index["files"]),
                 "logical_bytes": sum(item["bytes"] for item in index["files"]),
                 "unique_contents": len(objects),
                 "unique_bytes": sum(manifest[name]["bytes"] for name in objects),
                 "excluded_execution_products": len(index["excluded_execution_products"])}
        if index["statistics"] != stats:
            raise ValueError("evidence statistics do not match actual mappings")
        return index


def pack(config_path, output, max_bytes):
    config = json.loads(config_path.read_text())
    if config.get("inputs_frozen") is not True:
        raise ValueError("inputs are not yet frozen; no archive created")
    pending = output.with_name(output.name + ".partial")
    if output.exists() or pending.exists():
        raise FileExistsError("output or partial archive already exists")
    files, excluded, sources, manifest, objects = [], [], [], {}, {}
    source_ids, snapshots = set(), []
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(pending, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        def write(name, data):
            safe_path(name)
            if name in manifest:
                raise ValueError("duplicate output member")
            archive.writestr(name, data)
            manifest[name] = {"bytes": len(data), "sha256": digest(data)}

        def include(source, original, data, may_exclude):
            safe_path(original)
            sha = digest(data)
            entry = {"component": source["component"], "source": source["id"],
                     "path": original, "bytes": len(data), "sha256": sha}
            kind = evidence_kind(original, data)
            if kind == "excluded-execution-product":
                if not may_exclude:
                    raise ValueError("execution product found in sealed source archive: " + original)
                excluded.append({**entry, "reason": kind})
                return
            stored = f"records/sha256/{sha[:2]}/{sha}"
            if sha not in objects:
                write(stored, data)
                objects[sha] = len(data)
            elif objects[sha] != len(data):
                raise ValueError("content digest has inconsistent lengths")
            files.append({**entry, "kind": kind, "stored_path": stored,
                          "restore_path": restored_path(source["component"], source["id"], original)})

        for specification in config["sources"]:
            source = dict(specification)
            safe_label(source["component"])
            identifier = safe_label(source["id"])
            if identifier in source_ids:
                raise ValueError("duplicate source identity: " + identifier)
            source_ids.add(identifier)
            path = Path(source["path"])
            if path.is_symlink():
                raise ValueError("input source must not be a symlink")
            if source["kind"] == "directory":
                before = tree_inventory(path)
                if not path.is_dir():
                    raise ValueError("input directory missing")
                for name, identity in before.items():
                    data = (path / name).read_bytes()
                    if fingerprint(path / name) != identity:
                        raise ValueError("input changed while reading: " + name)
                    include(source, name, data, True)
                snapshots.append((path, before, True))
                source["original_files"] = len(before)
            else:
                before = fingerprint(path)
                actual_sha = file_digest(path)
                if actual_sha != source["sha256"] or before[2] != source["bytes"]:
                    raise ValueError("input identity mismatch: " + str(path))
                if source["kind"] == "compact":
                    # Expand an already verified compact container's logical
                    # files, not its records/ objects, so one restore recreates
                    # the actual original paths. Retain its metadata verbatim.
                    original_index = verify(path)
                    source["embedded_format"] = original_index["format"]
                    with zipfile.ZipFile(path) as original:
                        for entry in original_index["files"]:
                            name = entry["source"] + "/" + entry["path"]
                            include(source, name, original.read(entry["stored_path"]), False)
                        for entry in original_index["excluded_execution_products"]:
                            excluded.append({"component": source["component"], "source": source["id"],
                                             "path": entry["source"] + "/" + entry["path"],
                                             "bytes": entry["bytes"], "sha256": entry["sha256"],
                                             "reason": entry["reason"]})
                        for entry in original.infolist():
                            if not entry.filename.startswith("records/"):
                                include(source, "_compact/" + entry.filename, original.read(entry), False)
                elif source["kind"] == "zip":
                    with zipfile.ZipFile(path) as original:
                        seen = set()
                        for entry in original.infolist():
                            if entry.is_dir():
                                continue
                            if entry.filename in seen or stat.S_ISLNK(entry.external_attr >> 16):
                                raise ValueError("duplicate or symlink input ZIP member")
                            seen.add(entry.filename)
                            include(source, entry.filename, original.read(entry), False)
                elif source["kind"] == "file":
                    include(source, source.get("original_path", path.name), path.read_bytes(), False)
                else:
                    raise ValueError("unsupported source kind")
                snapshots.append((path, before, False))
            sources.append(source)
        for path, before, is_tree in snapshots:
            after = tree_inventory(path) if is_tree else fingerprint(path)
            if before != after:
                raise ValueError("input changed during packaging: " + str(path))
        index = {"format": FORMAT, "scope": config.get("scope", "evidence only"),
                 "notes": config.get("notes", []), "sources": sources, "files": files,
                 "excluded_execution_products": excluded,
                 "statistics": {"logical_files": len(files),
                                "logical_bytes": sum(item["bytes"] for item in files),
                                "unique_contents": len(objects), "unique_bytes": sum(objects.values()),
                                "excluded_execution_products": len(excluded)}}
        write("INDEX.json", json_bytes(index))
        write("INPUTS.json", json_bytes(config))
        write("README.md", README.encode())
        write("compact_evidence.py", Path(__file__).read_bytes())
        archive.writestr("MANIFEST.json", json_bytes(manifest))
    verified = verify(pending)
    if pending.stat().st_size > max_bytes:
        raise ValueError("archive exceeds byte cap; partial archive retained")
    # Same-directory hardlink publication never replaces an existing file.
    os.link(pending, output)
    pending.unlink()
    return {"integrity": "Pass", "path": str(output.resolve()),
            "bytes": output.stat().st_size, "sha256": file_digest(output),
            "statistics": verified["statistics"], "test_execution": "NotRun by packager"}


def restore(archive_path, destination, component=None):
    index = verify(archive_path)
    selected = [entry for entry in index["files"]
                if component is None or entry["component"] == component]
    if not selected:
        raise ValueError("component has no evidence files")
    destination.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(archive_path) as archive:
        for entry in selected:
            target = destination.joinpath(*safe_path(entry["restore_path"]).parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            data = archive.read(entry["stored_path"])
            if len(data) != entry["bytes"] or digest(data) != entry["sha256"]:
                raise ValueError("content changed after verification")
            with target.open("xb") as stream:
                stream.write(data)
    return {"restored_files": len(selected), "destination": str(destination.resolve())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    packing = commands.add_parser("pack")
    packing.add_argument("inputs", type=Path)
    packing.add_argument("output", type=Path)
    packing.add_argument("--max-bytes", type=int, default=50_000_000)
    checking = commands.add_parser("verify")
    checking.add_argument("archive", type=Path)
    restoring = commands.add_parser("restore")
    restoring.add_argument("archive", type=Path)
    restoring.add_argument("destination", type=Path)
    restoring.add_argument("--component")
    args = parser.parse_args()
    if args.command == "pack":
        result = pack(args.inputs, args.output, args.max_bytes)
    elif args.command == "restore":
        result = restore(args.archive, args.destination, args.component)
    else:
        result = {"integrity": "Pass", "statistics": verify(args.archive)["statistics"]}
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

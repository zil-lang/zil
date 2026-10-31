#!/usr/bin/env python3
"""Package frozen CI audit bytes without extracting a repository bundle.

This is packaging tooling, not original CI evidence. The required RESULT hash
must be supplied by the independent auditor after the input directory freezes.
No compiler, test runner, network request, or git mutation is invoked.

Example:
  python3 package_ci_receipts.py --source AUDIT_DIR --final-zip FINAL_ZIP \
    --destination NEW_DIR --result-sha256 SHA --final-zip-sha256 SHA
"""
import argparse
import datetime
import hashlib
import json
import lzma
import pathlib
import re
import shutil
import stat
import tarfile
import zipfile


BLOCK = 1024 * 1024
PRIVATE_DOMAIN = re.compile(rb"https?://[^\s\"'<>]*oaiusercontent[^\s\"'<>]*", re.I)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(BLOCK), b""):
            digest.update(block)
    return digest.hexdigest()


def file_record(path, name):
    metadata = path.stat()
    return {
        "path": name,
        "bytes": metadata.st_size,
        "mode": stat.S_IMODE(metadata.st_mode),
        "sha256": sha_file(path),
    }


def contains_download_metadata(value):
    if isinstance(value, dict):
        return any(
            (key == "download_url" and isinstance(child, str)
             and child.startswith(("https://", "http://")))
            or contains_download_metadata(child)
            for key, child in value.items()
        )
    if isinstance(value, list):
        return any(contains_download_metadata(child) for child in value)
    return False


def contains_private_transport(path):
    data = path.read_bytes()
    if PRIVATE_DOMAIN.search(data):
        return True
    if path.suffix == ".json":
        return contains_download_metadata(json.loads(data))
    return False


class HashReader:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()
        self.bytes = 0
        self.tail = b""
        self.private = False

    def read(self, size=-1):
        block = self.stream.read(size)
        self.digest.update(block)
        self.bytes += len(block)
        sample = self.tail + block
        self.private |= bool(PRIVATE_DOMAIN.search(sample))
        self.tail = sample[-4096:]
        return block


def verify_archive(path, records):
    expected = {record["path"]: record for record in records}
    require(len(expected) == len(records), "duplicate manifest member")
    seen = set()
    with tarfile.open(path, "r|xz") as archive:
        for member in archive:
            require(member.name in expected and member.name not in seen,
                    "unexpected or duplicate archive member")
            record = expected[member.name]
            seen.add(member.name)
            require(member.size == record["bytes"]
                    and member.mode == record["mode"],
                    "archive member size or mode mismatch")
            if member.isdir():
                require(record.get("type") == "directory", "directory mismatch")
                continue
            require(member.isfile(), "non-regular archive member")
            digest = hashlib.sha256()
            count = 0
            stream = archive.extractfile(member)
            for block in iter(lambda: stream.read(BLOCK), b""):
                digest.update(block)
                count += len(block)
            require(count == record["bytes"]
                    and digest.hexdigest() == record["sha256"],
                    "archive member bytes differ from original")
    require(seen == set(expected), "incomplete archive membership")
    return {
        "path": path.name,
        "bytes": path.stat().st_size,
        "sha256": sha_file(path),
        "members_verified": len(seen),
        "verification": "Completed archive reread: exact membership, original "
                        "uncompressed bytes, SHA256 and mode. No extraction.",
        "files": records,
    }


def package(args):
    source = args.source.resolve()
    destination = args.destination.resolve()
    final_zip = args.final_zip.resolve()
    require(source != destination and source not in destination.parents,
            "destination must be outside source audit directory")
    require(not (destination / "ARCHIVES.json").exists(),
            "completed package exists; original package will not be overwritten")
    original_result = source / "RESULT.json"
    require(sha_file(original_result) == args.result_sha256,
            "RESULT does not match auditor's frozen SHA256")
    result = json.loads(original_result.read_bytes())
    require(result.get("verdict") == "Pass" or result.get("status") == "Pass",
            "auditor has not recorded an explicit final Pass")
    require(not contains_private_transport(original_result),
            "RESULT contains transport metadata; cannot copy it into Git unchanged")
    inventory = []
    for path in sorted(source.rglob("*")):
        if path.is_file():
            require(not path.is_symlink(), "symlink in source audit directory")
            inventory.append(file_record(path, str(path.relative_to(source))))
    initial_files = {record["path"] for record in inventory}
    artifacts = json.loads((source / "artifacts-final.json").read_bytes())["artifacts"]
    metadata = {
        artifact["name"].rsplit("-", 1)[-1]: artifact
        for artifact in artifacts if not artifact["name"].endswith("-native")
    }
    require(final_zip.stat().st_size == metadata["final"]["size_in_bytes"],
            "final ZIP size differs from API metadata")
    final_digest = sha_file(final_zip)
    require(final_digest == args.final_zip_sha256
            and "sha256:" + final_digest == metadata["final"]["digest"],
            "final ZIP digest differs from frozen/API digest")
    destination.mkdir(parents=True, exist_ok=True)
    standalone = []
    for name, phase in [("RESULT.json", None), ("workspace.zip", "workspace"),
                        ("reference.zip", "reference")]:
        original = source / name
        record = file_record(original, name)
        if phase:
            require(record["bytes"] == metadata[phase]["size_in_bytes"]
                    and "sha256:" + record["sha256"] == metadata[phase]["digest"],
                    "phase ZIP does not match API digest")
        target = destination / name
        if target.exists():
            require(file_record(target, name) == record,
                    "existing standalone copy differs; refusing overwrite")
        else:
            shutil.copy2(original, target)
        require(file_record(target, name) == record, "standalone copy mismatch")
        if phase:
            record.update(api_artifact_id=metadata[phase]["id"],
                          api_digest=metadata[phase]["digest"])
        standalone.append(record)
    excluded = []
    audit_records = []
    for record in inventory:
        path = source / record["path"]
        if record["path"] in ("workspace.zip", "reference.zip"):
            continue
        if path.suffix == ".pyc":
            excluded.append({"sha256": record["sha256"], "reason":
                "Rebuildable Python bytecode cache; original audit source is "
                "archived. Not compiler execution evidence."})
        elif contains_private_transport(path):
            excluded.append({"sha256": record["sha256"], "reason":
                "Signed file-transfer URL or private download_url transport "
                "metadata. Transport-only scratch material, not compiler "
                "evidence; content intentionally excluded."})
        else:
            audit_records.append(record)
    require(any(record["path"] == "job.log" for record in audit_records),
            "complete job.log is not present")
    with tarfile.open(destination / "audit.tar.xz", "w:xz", preset=6) as archive:
        for record in audit_records:
            path = source / record["path"]
            info = archive.gettarinfo(str(path), arcname=record["path"])
            # gettarinfo can retain the st_mode file-type bits in memory.
            info.mode = stat.S_IMODE(info.mode)
            require(info.size == record["bytes"] and info.mode == record["mode"],
                    "audit source changed before archiving")
            with path.open("rb") as stream:
                archive.addfile(info, stream)
    print("Audit archive written.", flush=True)
    final_records = []
    bundle = None
    with zipfile.ZipFile(final_zip) as original, \
         lzma.open(destination / "final-receipts.tar.xz", "wb", preset=6) as packed, \
         tarfile.open(fileobj=packed, mode="w|") as archive:
        seen = set()
        for member in original.infolist():
            name = member.filename
            parts = pathlib.PurePosixPath(name)
            require(name not in seen and not parts.is_absolute()
                    and ".." not in parts.parts, "unsafe or duplicate ZIP path")
            seen.add(name)
            mode = (member.external_attr >> 16) & 0xffff
            permissions = stat.S_IMODE(mode)
            if name == "receipts/repository.bundle":
                bundle = {"path": name, "bytes": member.file_size,
                    "mode": permissions, "zip_crc32": f"{member.CRC:08x}",
                    "reason": "Explicitly excluded history bundle; retained in "
                    "original audited final ZIP. Bundle member was not "
                    "decompressed, extracted, or added to repackaged receipts."}
                continue
            require(stat.S_IFMT(mode) in (0, stat.S_IFREG, stat.S_IFDIR),
                    "non-regular ZIP member needs explicit handling")
            archive_name = name.rstrip("/") if member.is_dir() else name
            info = tarfile.TarInfo(archive_name)
            info.mode = permissions
            info.mtime = int(datetime.datetime(*member.date_time,
                                 tzinfo=datetime.timezone.utc).timestamp())
            record = {"path": archive_name, "bytes": member.file_size,
                      "mode": permissions, "zip_external_mode": mode}
            if member.is_dir():
                info.type = tarfile.DIRTYPE
                archive.addfile(info)
                record.update(type="directory", bytes=0,
                              source_zip_path=name,
                              sha256=hashlib.sha256(b"").hexdigest())
            else:
                info.size = member.file_size
                with original.open(member, "r") as stream:
                    reader = HashReader(stream)
                    archive.addfile(info, reader)
                    require(reader.bytes == member.file_size and not reader.private,
                            "ZIP member size/privacy check failed")
                    record.update(type="file", sha256=reader.digest.hexdigest(),
                                  zip_crc32=f"{member.CRC:08x}")
            final_records.append(record)
        source_member_count = len(seen)
    require(bundle is not None and len(final_records) + 1 == source_member_count,
            "final ZIP coverage is not exact")
    print("Final receipts streamed; bundle member was not decompressed.", flush=True)
    archives = [verify_archive(destination / "audit.tar.xz", audit_records),
                verify_archive(destination / "final-receipts.tar.xz", final_records)]
    final_inventory = {
        str(path.relative_to(source)) for path in source.rglob("*") if path.is_file()
    }
    require(initial_files == final_inventory, "source audit file set changed")
    for record in inventory:
        require(file_record(source / record["path"], record["path"]) == record,
                "source audit bytes changed during packaging")
    require(sha_file(destination / "RESULT.json") == args.result_sha256,
            "RESULT copy is not the auditor's original bytes")
    (destination / "EXCLUSIONS.json").write_text(json.dumps(excluded, indent=2) + "\n")
    manifest = {
        "schema_version": 1,
        "operation": "Lossless packaging of existing CI audit and receipts; "
                     "no test rerun or change to receipt identity, counts or conclusions.",
        "source_result_sha256": args.result_sha256,
        "source_audit_directory": str(source),
        "standalone_original_bytes": standalone,
        "archives": archives,
        "source_final_zip": {
            "sha256": final_digest, "bytes": final_zip.stat().st_size,
            "api_artifact_id": metadata["final"]["id"],
            "api_digest": metadata["final"]["digest"],
            "source_members": source_member_count,
            "included_members": len(final_records),
            "excluded_history_bundle": bundle,
        },
        "excluded_scratch_metadata": "EXCLUSIONS.json contains only original "
                                     "SHA256 and reason, never private transfer content.",
        "packaging_tool": file_record(pathlib.Path(__file__).resolve(),
                                      pathlib.Path(__file__).name),
        "result_unchanged": True, "phase_zips_unchanged": True,
        "new_compiler_tests_executed": False, "source_files_modified": False,
        "git_commit_created": False,
    }
    (destination / "ARCHIVES.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({
        "destination": str(destination),
        "total_bytes": sum(path.stat().st_size for path in destination.iterdir()
                           if path.is_file()),
        "result_sha256": args.result_sha256,
        "archives": [{key: value for key, value in entry.items() if key != "files"}
                     for entry in archives],
        "excluded_scratch_files": len(excluded),
    }, indent=2), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=pathlib.Path)
    parser.add_argument("--final-zip", required=True, type=pathlib.Path)
    parser.add_argument("--destination", required=True, type=pathlib.Path)
    parser.add_argument("--result-sha256", required=True)
    parser.add_argument("--final-zip-sha256", required=True)
    package(parser.parse_args())

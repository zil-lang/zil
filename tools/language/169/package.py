#!/usr/bin/env python3
"""Create a Git-derived source ZIP and a separate, bounded audit/recovery ZIP.

No generated compiler outputs or evidence enter the source ZIP. Historical Git
objects remain unchanged in the optional recovery bundle; current native outputs
are never selected as audit receipts. This tool proves packaging integrity only.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile
import tempfile
import zipfile


EXCLUDED_ROOTS = {"evidence", "target", "out", "run-logs", ".git", "__pycache__"}
NATIVE_SUFFIXES = {".exe", ".o", ".obj", ".a", ".lib", ".so", ".dll", ".dylib", ".wasm", ".bc"}
NATIVE_MAGICS = (b"\x7fELF", b"MZ", b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe",
                 b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")


def git(root, *args, binary=False):
    result = subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)
    return result.stdout if binary else result.stdout.decode().strip()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    path = PurePosixPath(name)
    if (not name or name in {".", ".."} or path.is_absolute() or path.as_posix() != name
            or any(part in {"", ".", ".."} for part in path.parts)
            or "\\" in name or ":" in name or any(ord(c) < 32 for c in name)):
        raise ValueError(f"nonportable archive path: {name}")
    return name


def native(name, data):
    return PurePosixPath(name).suffix.lower() in NATIVE_SUFFIXES or data.startswith(NATIVE_MAGICS)


def put(archive, name, data, mode=0o644):
    entry = zipfile.ZipInfo(safe_name(name), date_time=(2026, 10, 27, 0, 0, 0))
    entry.create_system = 3
    entry.external_attr = (stat.S_IFREG | mode) << 16
    entry.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(entry, data, compresslevel=9)


def source_inventory(root, commit):
    object_format = git(root, "rev-parse", "--show-object-format")
    expected = {}
    for row in git(root, "ls-tree", "-rz", commit, binary=True).split(b"\0"):
        if not row:
            continue
        head, raw_name = row.split(b"\t", 1)
        mode, kind, object_id = head.decode().split()
        name = safe_name(raw_name.decode())
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError(f"source delivery requires ordinary tracked files: {name}")
        expected[name] = (mode, object_id)
    archive_bytes = git(root, "archive", "--format=tar", commit, binary=True)
    source, excluded, seen = {}, [], set()
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:") as archive:
        for entry in archive:
            if entry.isdir():
                continue
            name = safe_name(entry.name)
            if not entry.isfile() or name not in expected or name in seen:
                raise ValueError(f"unexpected Git archive member: {name}")
            seen.add(name)
            if PurePosixPath(name).parts[0] in EXCLUDED_ROOTS:
                excluded.append(name)
                continue
            if name in {"PACKAGE.json", "SHA256SUMS"}:
                raise ValueError(f"delivery metadata must not replace tracked source: {name}")
            data = archive.extractfile(entry).read()
            blob_hash = hashlib.new(object_format, b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            if blob_hash != expected[name][1]:
                raise ValueError(f"Git export attributes transformed tracked bytes: {name}")
            if native(name, data):
                raise ValueError(f"native output unexpectedly tracked outside evidence: {name}")
            mode = 0o755 if expected[name][0] == "100755" else 0o644
            source[name] = (data, mode)
    if seen != set(expected):
        raise ValueError("Git export attributes omitted tracked files; inspect before delivery")
    return source, sorted(excluded)


def snapshot_history(root, directory, commit):
    mirror = directory / "repository.git"
    git_dir = Path(git(root, "rev-parse", "--git-common-dir"))
    if not git_dir.is_absolute():
        git_dir = (root / git_dir).resolve()
    subprocess.run(["git", "clone", "--mirror", "--no-hardlinks", str(git_dir), str(mirror)],
                   check=True, capture_output=True)
    # Preserve visible detached worktree commits without changing the source repo.
    worktrees = {"main": commit}
    for block in git(root, "worktree", "list", "--porcelain").split("\n\n"):
        records = dict(line.split(" ", 1) if " " in line else (line, "")
                       for line in block.splitlines())
        revision = records.get("HEAD")
        if not revision:
            continue
        name = Path(records["worktree"]).name
        safe_name(name)
        if name != "main":
            worktrees[name] = revision
        if "detached" in records:
            git(mirror, "update-ref", f"refs/heads/recovery/{name}", revision)
    refs = dict(row.split(" ", 1) for row in
                git(mirror, "for-each-ref", "--format=%(refname) %(objectname)").splitlines())
    if refs.get("refs/heads/main") != commit:
        raise ValueError("main changed while preparing recovery history")
    bundle = directory / "history.bundle"
    git(mirror, "bundle", "create", str(bundle), "--all")
    git(mirror, "bundle", "verify", str(bundle))
    commits = (git(mirror, "rev-list", "--all") + "\n").encode()
    bundle_bytes = bundle.read_bytes()
    manifest = {"source_commit": commit, "refs": refs, "worktrees": worktrees,
                "reachable_commits": len(commits.splitlines()), "commits_sha256": sha(commits),
                "bundle": {"file": "history.bundle", "bytes": len(bundle_bytes),
                           "sha256": sha(bundle_bytes)}}
    return {"history/history.bundle": bundle_bytes, "history/manifest.json": encoded(manifest),
            "history/commits.txt": commits}


def receipts(root, paths):
    selected = {}
    for relative in paths:
        name = safe_name(relative)
        path = root / name
        if (path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root)
                or any(parent.is_symlink() for parent in path.parents if parent != root)):
            raise ValueError(f"receipt must be an ordinary existing file: {name}")
        data = path.read_bytes()
        if (path.suffix not in {".json", ".log", ".md", ".txt", ".zil", ".rs", ".py", ".toml", ".sh"}
                or native(name, data) or len(data) > 2 * 1024 * 1024):
            raise ValueError(f"receipt is executable or exceeds 2 MiB: {name}")
        selected["receipts/" + name] = data
    return selected


def build(args):
    root = args.root.resolve()
    output, audit_output = args.output.resolve(), args.audit_output.resolve()
    if output == audit_output or output.exists() or audit_output.exists():
        raise ValueError("outputs must be distinct new files; prior deliveries are never overwritten")
    if git(root, "status", "--porcelain") or git(root, "branch", "--show-current") != "main":
        raise ValueError("delivery requires a clean main checkout")
    commit = git(root, "rev-parse", "HEAD")
    source, excluded = source_inventory(root, commit)
    source_record = {"commit": commit, "branch": "main", "dirty": False,
                     "crates_tree": git(root, "rev-parse", "HEAD:crates"),
                     "normative_hash": json.loads((root / "data/normative-inputs.json").read_text())["normative_hash"]}
    selected = receipts(root, args.receipt)
    with tempfile.TemporaryDirectory(prefix="zil-language169-package-") as temporary:
        temp = Path(temporary)
        selected.update(snapshot_history(root, temp, commit))
        selected["history/restore.py"] = (root / "tools/language/164/restore.py").read_bytes()
        audit_manifest = {"schema_version": 1, "source": source_record,
                          "scope": "Selected compact receipts and unchanged Git history; no new test execution",
                          "current_executables_included": False,
                          "files": [{"path": name, "bytes": len(data), "sha256": sha(data)}
                                    for name, data in sorted(selected.items())]}
        with zipfile.ZipFile(temp / "audit.zip", "w") as archive:
            for name, data in sorted(selected.items()):
                put(archive, name, data)
            put(archive, "MANIFEST.json", encoded(audit_manifest))
        audit_data = (temp / "audit.zip").read_bytes()
        if len(audit_data) > args.max_audit_mb * 1_000_000:
            raise ValueError("audit exceeds requested size cap; choose fewer receipts, do not omit silently")
        package = {"format": "zil-source/1", "schema_version": 2, "source": source_record,
                   "scope": "Git-tracked source, tests, fixtures, configuration and documentation only",
                   "retention": {"policy": "git-source-without-evidence/1", "evidence_files_kept": 0,
                                 "excluded_roots": sorted(EXCLUDED_ROOTS), "excluded_tracked_paths": excluded,
                                 "native_outputs_included": False},
                   "companions": {"audit": {"status": "included", "file": audit_output.name,
                                              "bytes": len(audit_data), "sha256": sha(audit_data)}}}
        checksums = []
        with zipfile.ZipFile(temp / "source.zip", "w") as archive:
            for name, (data, mode) in sorted(source.items()):
                put(archive, "zil/" + name, data, mode)
                checksums.append(sha(data) + "  " + name + "\n")
            metadata = encoded(package)
            put(archive, "zil/PACKAGE.json", metadata)
            checksums.append(sha(metadata) + "  PACKAGE.json\n")
            put(archive, "zil/SHA256SUMS", "".join(checksums).encode())
        if git(root, "rev-parse", "HEAD") != commit or git(root, "status", "--porcelain"):
            raise ValueError("source changed while packaging; nothing published")
        for path in (temp / "source.zip", temp / "audit.zip"):
            with zipfile.ZipFile(path) as archive:
                if archive.testzip() is not None:
                    raise ValueError("ZIP CRC verification failed")
        result = {"status": "Pass", "scope": "packaging only", "source_commit": commit,
                  "source_files": len(source) + 2, "evidence_files_in_source": 0,
                  "source": {}, "audit": {}}
        for kind, path, destination in (("source", temp / "source.zip", output),
                                        ("audit", temp / "audit.zip", audit_output)):
            destination.parent.mkdir(parents=True, exist_ok=True)
            data = path.read_bytes()
            with destination.open("xb") as handle:
                handle.write(data)
            result[kind] = {"file": str(destination), "bytes": len(data), "sha256": sha(data)}
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--audit-output", required=True, type=Path)
    parser.add_argument("--receipt", action="append", default=[], help="explicit repository-relative compact receipt")
    parser.add_argument("--max-audit-mb", type=int, default=50)
    args = parser.parse_args()
    if args.max_audit_mb < 1:
        parser.error("audit size cap must be positive")
    try:
        result = build(args)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(json.dumps({"status": "Fail", "error": str(error)}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

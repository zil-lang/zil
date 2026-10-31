"""Restore checked Git objects and atomically publish only approved task refs.

Legacy: .construction9/tasks.bundle.b64 plus manifest.bundle_sha256.
Chunked: ordered manifest.bundle_files entries with path, size, sha256 and an
optional encoding (base64 by default, or raw). Part size and SHA-256 always
describe the decoded binary bytes. bundle_sha256 describes their concatenation.
Optional bundle_size describes its binary byte length.

No arguments retains the existing CI publication entry point. --verify-only
imports into local staging refs and writes a verification receipt without any
origin lookup, network fetch, or push.
"""
import argparse
import base64
import binascii
import contextlib
import hashlib
import json
import pathlib
import re
import subprocess
import tempfile


ALLOWED = frozenset({"task/228", "task/229", "task/228-consumer", "task/228-229"})


def require(condition, message):
    if not condition:
        raise ValueError(message)


def hex_string(value, length):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{%d}" % length, value)


def byte_size(value):
    return type(value) is int and value >= 0


def unique_json(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key: " + key)
        result[key] = value
    return result


def git(root, *args):
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def run_git(root, *args):
    subprocess.run(["git", *args], cwd=root, check=True)


def payload_path(root, name):
    require(isinstance(name, str) and "\\" not in name, "invalid payload path")
    relative = pathlib.PurePosixPath(name)
    require(
        not relative.is_absolute()
        and len(relative.parts) >= 2
        and relative.parts[0] == ".construction9"
        and ".." not in relative.parts,
        "payload must remain below .construction9",
    )
    current = root
    for part in relative.parts:
        current = current / part
        require(not current.is_symlink(), "symlink payload path rejected")
    require(current.is_file(), "missing regular payload file: " + name)
    return current


def validate_manifest(manifest):
    require(isinstance(manifest, dict), "manifest must be an object")
    require(type(manifest.get("schema_version")) is int and manifest["schema_version"] in (1, 2), "unsupported manifest schema")
    require(hex_string(manifest.get("bundle_sha256"), 64), "invalid bundle SHA-256")
    if "bundle_size" in manifest:
        require(byte_size(manifest["bundle_size"]), "invalid bundle byte size")
    targets = manifest.get("targets")
    require(isinstance(targets, list) and 0 < len(targets) <= len(ALLOWED), "invalid target count")
    seen = set()
    for target in targets:
        require(isinstance(target, dict), "target must be an object")
        branch, expected = target.get("branch"), target.get("sha")
        require(isinstance(branch, str) and branch in ALLOWED, "target branch is not approved")
        require(branch not in seen, "duplicate target branch")
        require(hex_string(expected, 40), "invalid exact target commit SHA")
        seen.add(branch)
    if "bundle_files" not in manifest:
        return [{"path": ".construction9/tasks.bundle.b64", "encoding": "base64"}]
    parts = manifest["bundle_files"]
    require(isinstance(parts, list) and bool(parts), "bundle_files must be a nonempty ordered array")
    seen = set()
    for part in parts:
        require(isinstance(part, dict), "bundle part must be an object")
        name = part.get("path")
        require(isinstance(name, str) and name not in seen, "invalid or duplicate part path")
        require(byte_size(part.get("size")), "part size must be decoded binary byte length")
        require(hex_string(part.get("sha256"), 64), "invalid decoded part SHA-256")
        require(part.get("encoding", "base64") in ("base64", "raw"), "unsupported part encoding")
        seen.add(name)
    return parts


@contextlib.contextmanager
def assemble_bundle(root, manifest, parts):
    directory = root / ".construction9"
    require(directory.is_dir() and not directory.is_symlink(), "invalid transport directory")
    name = None
    try:
        with tempfile.NamedTemporaryFile(prefix="assembled-", suffix=".bundle", dir=directory, delete=False) as output:
            name = pathlib.Path(output.name)
            total_hash, total_size, receipts = hashlib.sha256(), 0, []
            for part in parts:
                path = payload_path(root, part["path"])
                encoding = part.get("encoding", "base64")
                stored = path.read_bytes()
                if encoding == "base64":
                    try:
                        data = base64.b64decode(stored, validate=True)
                    except (ValueError, binascii.Error) as error:
                        raise ValueError("invalid base64 payload: " + part["path"]) from error
                else:
                    data = stored
                digest = hashlib.sha256(data).hexdigest()
                if "bundle_files" in manifest:
                    require(len(data) == part["size"], "decoded part size mismatch: " + part["path"])
                    require(digest == part["sha256"], "decoded part SHA-256 mismatch: " + part["path"])
                output.write(data)
                total_hash.update(data)
                total_size += len(data)
                receipts.append({"path": part["path"], "encoding": encoding, "size": len(data), "sha256": digest})
            require(total_hash.hexdigest() == manifest["bundle_sha256"], "concatenated bundle SHA-256 mismatch")
            if "bundle_size" in manifest:
                require(total_size == manifest["bundle_size"], "concatenated bundle size mismatch")
        yield name, receipts, total_size
    finally:
        if name is not None:
            name.unlink(missing_ok=True)


def parse_refs(text):
    refs = {}
    for line in text.splitlines():
        columns = line.split()
        require(len(columns) == 2 and hex_string(columns[0], 40), "invalid Git ref advertisement")
        sha, ref = columns
        require(ref not in refs, "duplicate Git ref advertisement")
        refs[ref] = sha
    return refs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    root = pathlib.Path(git(pathlib.Path.cwd(), "rev-parse", "--show-toplevel")).resolve()
    manifest_path = payload_path(root, ".construction9/manifest.json")
    manifest = json.loads(manifest_path.read_text(), object_pairs_hook=unique_json)
    parts = validate_manifest(manifest)
    targets = manifest["targets"]
    expected_heads = {"refs/heads/" + target["branch"]: target["sha"] for target in targets}
    with assemble_bundle(root, manifest, parts) as (bundle, part_receipts, bundle_size):
        run_git(root, "bundle", "verify", str(bundle))
        require(parse_refs(git(root, "bundle", "list-heads", str(bundle))) == expected_heads,
                "bundle heads must exactly match the approved manifest targets")
        receipts, refspecs = [], []
        for target in targets:
            branch, expected = target["branch"], target["sha"]
            ref = "refs/heads/" + branch
            local_ref = "refs/construction9/" + manifest["bundle_sha256"] + "/" + branch
            run_git(root, "fetch", "--no-tags", str(bundle), ref + ":" + local_ref)
            actual = git(root, "rev-parse", local_ref)
            require(actual == expected and git(root, "cat-file", "-t", actual) == "commit", "imported target is not the exact commit")
            old = None
            if not args.verify_only:
                remote = parse_refs(git(root, "ls-remote", "--heads", "origin", ref))
                require(set(remote).issubset({ref}), "unexpected remote ref advertisement")
                old = remote.get(ref)
                if old:
                    run_git(root, "fetch", "--no-tags", "origin", ref)
                    run_git(root, "merge-base", "--is-ancestor", old, actual)
            refspecs.append(actual + ":" + ref)
            receipts.append({"branch": branch, "sha": actual, "previous": old, "local_ref": local_ref})
        if not args.verify_only:
            # Exact four-name allowlist; never force, never main; atomic or fail.
            run_git(root, "push", "--atomic", "origin", *refspecs)
            for row in receipts:
                ref = "refs/heads/" + row["branch"]
                require(parse_refs(git(root, "ls-remote", "--heads", "origin", ref)) == {ref: row["sha"]}, "published target verification failed")
        result = {"status": "Pass", "mode": "verify-only" if args.verify_only else "publication",
                  "remote_access": not args.verify_only, "bundle_sha256": manifest["bundle_sha256"],
                  "bundle_size": bundle_size, "bundle_files": part_receipts, "targets": receipts}
        receipt_name = "construction9-verification.json" if args.verify_only else "construction9-publication.json"
        (root / receipt_name).write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

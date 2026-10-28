#!/usr/bin/env python3
"""Export a pinned Git bundle or import it without changing a working branch."""
import argparse
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import tempfile


class BundleError(Exception):
    pass


def git(root, *args, allowed=(0,)):
    result = subprocess.run(["git", "-C", str(root), *map(str, args)],
                            capture_output=True, text=True, encoding="utf-8", errors="replace")
    if result.returncode not in allowed:
        raise BundleError(f"git {' '.join(map(str, args))}: {result.stderr.strip() or result.stdout.strip()}")
    return result


def oid(value):
    return isinstance(value, str) and re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", value)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def header(path):
    refs, prerequisites, capabilities = {}, [], []
    with path.open("rb") as stream:
        if stream.readline(100) not in (b"# v2 git bundle\n", b"# v3 git bundle\n"):
            raise BundleError("unsupported Git bundle header")
        total = 0
        while True:
            line = stream.readline(65537)
            total += len(line)
            if total > 1048576 or len(line) > 65536 or not line.endswith(b"\n"):
                raise BundleError("invalid or oversized Git bundle header")
            if line == b"\n":
                break
            text = line[:-1].decode("utf-8")
            if text.startswith("@"):
                if text not in ("@object-format=sha1", "@object-format=sha256"):
                    raise BundleError("filtered or unknown bundle capability is not supported")
                capabilities.append(text[1:])
                continue
            revision, separator, name = text.partition(" ")
            if not separator:
                raise BundleError("invalid bundle ref or prerequisite")
            if revision.startswith("-"):
                if not oid(revision[1:]):
                    raise BundleError("invalid prerequisite object id")
                prerequisites.append(revision[1:])
            else:
                if not oid(revision) or name in refs:
                    raise BundleError("invalid or duplicate advertised ref")
                refs[name] = revision
    return {"refs": refs, "prerequisites": sorted(prerequisites), "capabilities": capabilities}


def export_bundle(args):
    root, output = args.repo.resolve(), args.output.resolve()
    metadata_path = output.with_name(output.name + ".json")
    if output.exists() or metadata_path.exists():
        raise BundleError("refusing to replace an existing bundle or manifest")
    if not args.source_ref.startswith("refs/heads/"):
        raise BundleError("source ref must be an explicit refs/heads/ branch")
    git(root, "check-ref-format", args.source_ref)
    source = git(root, "rev-parse", "--verify", args.source_ref + "^{commit}").stdout.strip()
    if args.expect_commit is not None and args.expect_commit != source:
        raise BundleError("source ref differs from --expect-commit")
    if git(root, "rev-parse", "HEAD").stdout.strip() != source:
        raise BundleError("export from a worktree checked out at the source commit")
    if git(root, "status", "--porcelain=v1", "--untracked-files=all").stdout:
        raise BundleError("export requires a clean source worktree")
    if git(root, "rev-parse", "--is-shallow-repository").stdout.strip() != "false":
        raise BundleError("a shallow repository cannot certify complete history")
    base = None
    if args.base:
        base = git(root, "rev-parse", "--verify", args.base + "^{commit}").stdout.strip()
        if base == source:
            raise BundleError("incremental bundle would contain no new commits")
        if git(root, "merge-base", "--is-ancestor", base, source, allowed=(0, 1)).returncode:
            raise BundleError("incremental base is not an ancestor of the source")
    if args.max_bytes < 1:
        raise BundleError("--max-bytes must be positive")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="zil-bundle-") as directory:
        temporary = pathlib.Path(directory) / "history.bundle"
        revisions = [args.source_ref] + (["^" + base] if base else [])
        git(root, "bundle", "create", temporary, *revisions)
        actual = header(temporary)
        if actual["refs"] != {args.source_ref: source}:
            raise BundleError("source ref changed while producing the bundle")
        if not base and actual["prerequisites"]:
            raise BundleError("full bundle unexpectedly requires prior commits")
        git(root, "bundle", "verify", temporary)
        if temporary.stat().st_size > args.max_bytes:
            raise BundleError(f"bundle exceeds {args.max_bytes} bytes; no delivery was published")
        if (git(root, "rev-parse", args.source_ref).stdout.strip() != source
                or git(root, "rev-parse", "HEAD").stdout.strip() != source
                or git(root, "status", "--porcelain=v1", "--untracked-files=all").stdout):
            raise BundleError("source changed during export")
        manifest = {
            "format": "zil-offline-bundle-v1", "kind": "incremental" if base else "full",
            "source_ref": args.source_ref, "source_commit": source,
            "source_tree": git(root, "rev-parse", source + "^{tree}").stdout.strip(),
            "excluded_base": base, "advertised_refs": actual["refs"],
            "prerequisites": actual["prerequisites"], "capabilities": actual["capabilities"],
            "bundle": {"file": output.name, "bytes": temporary.stat().st_size,
                       "sha256": digest(temporary)},
            "scope": "Only the named branch and its reachable, unchanged Git history; no uncommitted files or unrelated refs.",
        }
        manifest_temp = pathlib.Path(directory) / "manifest.json"
        manifest_temp.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        created = []
        try:
            for source_file, destination in ((temporary, output), (manifest_temp, metadata_path)):
                # Exclusive creation rejects a concurrent writer instead of overwriting it.
                with destination.open("xb") as target:
                    created.append(destination)
                    with source_file.open("rb") as stream:
                        shutil.copyfileobj(stream, target)
        except BaseException:
            for destination in reversed(created):
                destination.unlink()
            raise
    return {"status": "Exported", "bundle_path": str(output), "manifest": str(metadata_path), **manifest}


def import_bundle(args):
    root, bundle = args.repo.resolve(), args.bundle.resolve()
    metadata_path = args.manifest.resolve() if args.manifest else bundle.with_name(bundle.name + ".json")
    manifest = json.loads(metadata_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or manifest.get("format") != "zil-offline-bundle-v1":
        raise BundleError("unsupported bundle manifest")
    source = manifest.get("source_commit")
    source_tree = manifest.get("source_tree")
    source_ref = manifest.get("source_ref")
    details = manifest.get("bundle")
    if (not oid(source) or not oid(source_tree) or not isinstance(source_ref, str) or not source_ref.startswith("refs/heads/")
            or not isinstance(details, dict) or not re.fullmatch(r"[0-9a-f]{64}", str(details.get("sha256", "")))):
        raise BundleError("invalid source identity or bundle digest")
    git(root, "check-ref-format", source_ref)
    if details.get("bytes") != bundle.stat().st_size or details["sha256"] != digest(bundle):
        raise BundleError("bundle size or SHA256 differs from the manifest")
    actual = header(bundle)
    if (actual["refs"] != {source_ref: source} or manifest.get("advertised_refs") != actual["refs"]
            or manifest.get("prerequisites") != actual["prerequisites"]
            or manifest.get("capabilities") != actual["capabilities"]):
        raise BundleError("bundle header differs from the manifest")
    if manifest.get("kind") not in ("full", "incremental"):
        raise BundleError("invalid bundle kind")
    if manifest["kind"] == "full" and actual["prerequisites"]:
        raise BundleError("a full bundle cannot require prior commits")
    git(root, "bundle", "verify", bundle)
    destination = "refs/remotes/zil-language/" + source
    prior = git(root, "rev-parse", "--verify", destination, allowed=(0, 128)).stdout.strip()
    if prior and prior != source:
        raise BundleError("immutable destination ref already names another object")
    if prior:
        if git(root, "rev-parse", source + "^{tree}").stdout.strip() != source_tree:
            raise BundleError("source tree differs from the manifest")
        return {"status": "AlreadyImported", "source_commit": source, "destination_ref": destination}
    # No refspec destination and no FETCH_HEAD write: fetch objects only. A subsequent
    # compare-and-swap creates precisely one immutable ref, without touching any branch.
    git(root, "fetch", "--no-tags", "--no-write-fetch-head", "--recurse-submodules=no", bundle, source_ref)
    if details["sha256"] != digest(bundle):
        raise BundleError("bundle changed during import; no destination ref was created")
    git(root, "cat-file", "-e", source + "^{commit}")
    if git(root, "rev-parse", source + "^{tree}").stdout.strip() != source_tree:
        raise BundleError("source tree differs from the manifest")
    update = git(root, "update-ref", destination, source, "0" * len(source), allowed=(0, 128))
    if update.returncode:
        if git(root, "rev-parse", "--verify", destination, allowed=(0, 128)).stdout.strip() != source:
            raise BundleError("destination ref changed concurrently; it was not overwritten")
    return {"status": "Imported", "source_commit": source, "destination_ref": destination,
            "note": "Working branch, index, files and remotes were not changed. Merge separately after review."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=pathlib.Path, default=pathlib.Path.cwd())
    commands = parser.add_subparsers(dest="command", required=True)
    export = commands.add_parser("export", help="create a full or explicit-base incremental bundle")
    export.add_argument("--output", required=True, type=pathlib.Path)
    export.add_argument("--source-ref", default="refs/heads/main")
    export.add_argument("--expect-commit")
    export.add_argument("--base", help="exclude this commit and its ancestors")
    export.add_argument("--max-bytes", type=int, default=50_000_000)
    importer = commands.add_parser("import", help="verify and fetch to an immutable language ref")
    importer.add_argument("--bundle", required=True, type=pathlib.Path)
    importer.add_argument("--manifest", type=pathlib.Path)
    args = parser.parse_args()
    try:
        result = export_bundle(args) if args.command == "export" else import_bundle(args)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    except (BundleError, OSError, ValueError, UnicodeError) as error:
        print(json.dumps({"status": "Failed", "error": str(error)}, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Restore a verified local language-stage bundle without network or overwrite."""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess


def run(*command):
    subprocess.run([str(part) for part in command], check=True)


parser = argparse.ArgumentParser()
parser.add_argument("--manifest", required=True, type=pathlib.Path)
parser.add_argument("--destination", required=True, type=pathlib.Path)
args = parser.parse_args()
manifest_path = args.manifest.resolve()
manifest = json.loads(manifest_path.read_text())
source_commit = manifest.get("source_commit", "")
if (not re.fullmatch(r"[0-9a-f]{40}", source_commit)
        or not manifest.get("worktrees") or not manifest.get("refs")
        or manifest["worktrees"].get("main") != source_commit
        or manifest["refs"].get("refs/heads/main") != source_commit):
    parser.error("source_commit, main worktree and main ref must name the same nonempty source")
bundle = manifest_path.parent / manifest["bundle"]["file"]
destination = args.destination.resolve()
if destination.exists():
    parser.error(f"refusing to overwrite {destination}")
with open(bundle, "rb") as stream:
    actual_hash = hashlib.file_digest(stream, "sha256").hexdigest()
if actual_hash != manifest["bundle"]["sha256"]:
    parser.error("bundle SHA256 mismatch")
for name, revision in manifest["worktrees"].items():
    if pathlib.PurePath(name).name != name or name in (".", "..") or not re.fullmatch(r"[0-9a-f]{40}", revision):
        parser.error("invalid worktree identity in manifest")
destination.mkdir(parents=True)
repository = destination / "repository.git"
run("git", "clone", "--mirror", bundle, repository)
run("git", "--git-dir", repository, "fsck", "--full")
refs = subprocess.check_output(["git", "--git-dir", str(repository), "for-each-ref",
                               "--format=%(refname) %(objectname)"], text=True)
actual_refs = dict(line.split(" ", 1) for line in refs.splitlines())
if set(actual_refs) != set(manifest["refs"]):
    raise RuntimeError("restored refs differ from the complete manifest")
for name, revision in manifest["refs"].items():
    if actual_refs.get(name) != revision:
        raise RuntimeError(f"ref differs: {name}")
for name, revision in manifest["worktrees"].items():
    if name == "main" and actual_refs.get("refs/heads/main") == revision:
        run("git", "--git-dir", repository, "worktree", "add", destination / name, "main")
    else:
        run("git", "--git-dir", repository, "worktree", "add", "--detach", destination / name, revision)
    actual = subprocess.check_output(["git", "-C", str(destination / name), "rev-parse", "HEAD"], text=True).strip()
    if actual != revision:
        raise RuntimeError(f"worktree differs: {name}")
print(json.dumps({"status": "Pass", "source_commit": manifest["source_commit"],
                  "destination": str(destination), "worktrees": manifest["worktrees"],
                  "note": "Compiler toolchains and caches are installed separately; no GitHub access was used."}, indent=2))

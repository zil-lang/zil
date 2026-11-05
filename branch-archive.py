#!/usr/bin/env python3
"""Preserve ZIL refs and source WIP in one ancestry-only archive branch.

Creation writes Git objects and refs/heads/archive/source-history, but never
edits a live worktree or index. It never removes a branch. Run --plan first.
"""

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ARCHIVE = "refs/heads/archive/source-history"
SOURCE_DIRS = {"crates", "tests", "examples", "changes", "docs", "tools", "lib", ".task", "spec", "policy"}
SOURCE_SUFFIXES = {".rs", ".zil", ".md", ".json", ".py", ".mjs", ".toml", ".ebnf", ".sh", ".c", ".h"}


def git(repo, *args, input_data=None, env=None, check=True):
    process_env = os.environ.copy()
    for inherited in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"):
        process_env.pop(inherited, None)
    process_env.update({"GIT_OPTIONAL_LOCKS": "0", **(env or {})})
    process = subprocess.run(
        ["git", "-C", str(repo), *args], input=input_data, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        env=process_env, check=False,
    )
    if check and process.returncode:
        raise RuntimeError(f"git {' '.join(args)}: {process.stderr.strip()}")
    return process


def oid(repo, object_name):
    result = git(repo, "rev-parse", "--verify", object_name, check=False)
    return result.stdout.strip() if result.returncode == 0 else None


def source_untracked(path):
    relative = Path(path)
    return relative.parts[0] in SOURCE_DIRS and relative.suffix in SOURCE_SUFFIXES


def worktree_fingerprint(path):
    """Detect edits during capture, including same-status content rewrites."""
    digest = hashlib.sha256()
    for command in [("rev-parse", "HEAD"), ("status", "--porcelain=v1", "-z", "--untracked-files=all")]:
        digest.update(git(path, *command).stdout.encode())
    index = Path(git(path, "rev-parse", "--git-path", "index").stdout.strip())
    if not index.is_absolute():
        index = path / index
    if index.exists():
        digest.update(index.read_bytes())
    changed = [name for name in git(path, "diff", "--name-only", "-z", "HEAD").stdout.split("\0") if name]
    untracked = [name for name in git(path, "ls-files", "-z", "--others", "--exclude-standard").stdout.split("\0") if name and source_untracked(name)]
    for name in sorted(set(changed + untracked)):
        digest.update(name.encode())
        file = path / name
        if file.is_file():
            with file.open("rb") as handle:
                for block in iter(lambda: handle.read(1024 * 1024), b""):
                    digest.update(block)
        else:
            digest.update(b"<absent>")
    return digest.hexdigest()


def worktrees(repo):
    output = git(repo, "worktree", "list", "--porcelain").stdout
    result = []
    for block in output.strip().split("\n\n"):
        if not block:
            continue
        fields = block.splitlines()
        path = Path(fields[0].removeprefix("worktree "))
        head = next(line.split(" ", 1)[1] for line in fields if line.startswith("HEAD "))
        result.append((path, head))
    return result


def refs(repo):
    local = {}
    for line in git(repo, "for-each-ref", "--format=%(refname) %(objectname)").stdout.splitlines():
        name, sha = line.split(" ", 1)
        if name != ARCHIVE:
            local[name] = sha
    remote = {}
    for line in git(repo, "ls-remote", "--refs", "origin").stdout.splitlines():
        sha, name = line.split("\t", 1)
        remote[name] = sha
    return local, remote


def commit(repo, tree, parents, message):
    return git(repo, "commit-tree", tree, *[part for sha in parents for part in ("-p", sha)], input_data=message).stdout.strip()


def snapshot_worktree(repo, path, head, stamp, create):
    status = git(path, "status", "--porcelain=v1", "--untracked-files=all").stdout
    untracked = [name for name in git(path, "ls-files", "-z", "--others", "--exclude-standard").stdout.split("\0") if name]
    included = [name for name in untracked if source_untracked(name)]
    excluded = [name for name in untracked if name not in included]
    row = {"path": str(path), "head": head, "status": status.splitlines(),
           "included_untracked_source": included, "excluded_untracked": excluded}
    if not create or not status:
        return row, []
    parents = []
    # GIT_INDEX_FILE points outside the live Git directory. Neither read-tree
    # nor add can change the user's staged selection or working files.
    with tempfile.TemporaryDirectory(prefix="branch-archive-index-", dir=Path(__file__).parent) as directory:
        live_index = Path(git(path, "rev-parse", "--git-path", "index").stdout.strip())
        if not live_index.is_absolute():
            live_index = path / live_index
        with tempfile.NamedTemporaryFile(dir=directory, delete=False) as temporary:
            index_path = Path(temporary.name)
        index_path.unlink()
        env = dict(os.environ, GIT_INDEX_FILE=str(index_path))
        git(path, "read-tree", head, env=env)
        head_tree = oid(path, head + "^{tree}")
        if live_index.exists():
            # Read the live index through a separate command, never through the
            # temporary index. An unmerged index has no tree and must fail.
            if git(path, "ls-files", "-u").stdout:
                raise RuntimeError(f"unmerged live index at {path}; resolve or preserve it separately")
            copied_index = Path(directory) / "live-index-copy"
            shutil.copy2(live_index, copied_index)
            copied_env = dict(os.environ, GIT_INDEX_FILE=str(copied_index))
            index_tree = git(path, "write-tree", env=copied_env).stdout.strip()
            row["index_tree"] = index_tree
            if index_tree != head_tree:
                index_commit = commit(repo, index_tree, [head], f"Preserve staged index for {path} at {stamp}\n")
                row["index_commit"] = index_commit
                parents.append(index_commit)
        git(path, "add", "-u", env=env)
        # A path added to the live index is absent from HEAD, so add -u alone
        # cannot capture its current working-file version. The staged blob is
        # already preserved by index_commit above.
        staged_new = [name for name in git(path, "diff", "--cached", "--name-only", "--diff-filter=AR", "-z").stdout.split("\0") if name]
        present_new = [name for name in staged_new if (path / name).exists()]
        if present_new:
            git(path, "add", "-A", "--", *present_new, env=env)
        if included:
            git(path, "add", "--", *included, env=env)
        work_tree = git(path, "write-tree", env=env).stdout.strip()
        row["work_tree"] = work_tree
        if work_tree != head_tree or parents:
            # HEAD remains a parent even when staged and worktree trees differ.
            wip = commit(repo, work_tree, [head, *parents], f"Preserve source working tree for {path} at {stamp}\n")
            row["wip_commit"] = wip
            parents.append(wip)
    return row, parents


def archive_tree(repo, manifest, script_path):
    readme = (
        "# Source history archive\n\nThis branch preserves source ref tips and staged/working-tree snapshots "
        "as commit parents. It is not accepted source or a test result.\n\n"
        "Restore a preserved commit with `git branch NAME SHA`; inspect manifest.json "
        "for exact source refs, worktrees, exclusions, and WIP commits. "
        "Older ref names remain in prior manifests via `git log --first-parent archive/source-history`. "
        "Push and verify this archive ref before removing any old source ref.\n"
    )
    with tempfile.TemporaryDirectory(prefix="branch-archive-tree-", dir=Path(__file__).parent) as directory:
        index = Path(directory) / "index"
        env = dict(os.environ, GIT_INDEX_FILE=str(index))
        git(repo, "read-tree", "--empty", env=env)
        files = {"README.md": readme.encode(),
                 "manifest.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode(),
                 "branch-archive.py": script_path.read_bytes()}
        for name, data in files.items():
            blob = git(repo, "hash-object", "-w", "--stdin", input_data=data.decode()).stdout.strip()
            git(repo, "update-index", "--add", "--cacheinfo", "100644", blob, name, env=env)
        return git(repo, "write-tree", env=env).stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("/workspace/zil"))
    parser.add_argument("--receipt", type=Path, default=Path("/workspace/.zil-env/branch-archive-receipt.json"))
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--plan", action="store_true", help="read-only inventory (default)")
    modes.add_argument("--create", action="store_true", help="write archive objects and update the sole archive ref")
    parser.add_argument("--push", action="store_true", help="with --create, push the archive ref normally to origin")
    args = parser.parse_args()
    if args.push and not args.create:
        parser.error("--push requires --create")
    repo = args.repo.resolve()
    local, remote = refs(repo)
    stamp = dt.datetime.now(dt.timezone.utc).isoformat()
    local_old = oid(repo, ARCHIVE)
    old = local_old or remote.get(ARCHIVE)
    before = {str(path): worktree_fingerprint(path) for path, _ in worktrees(repo)}
    rows = []
    wip = []
    for path, head in worktrees(repo):
        row, parents = snapshot_worktree(repo, path, head, stamp, args.create)
        rows.append(row)
        wip.extend(parents)
    raw_tips = set([*local.values(), *remote.values(), *(row["head"] for row in rows), *wip])
    peeled = {sha: oid(repo, sha + "^{commit}") for sha in raw_tips}
    missing = sorted(sha for sha, target in peeled.items() if not target)
    tips = sorted(set(target for target in peeled.values() if target))
    independent = git(repo, "merge-base", "--independent", *tips).stdout.splitlines() if tips else []
    independent.sort()
    parents = [old] if old else []
    parents.extend(sha for sha in independent if sha != old)
    manifest = {"format": 1, "created_utc": stamp, "prior_archive": old,
                "local_refs": local, "remote_refs": remote, "worktrees": rows,
                "missing_commit_objects": missing, "commit_tips": tips,
                "commit_parents": parents,
                "scope": "Git source history preservation only; not acceptance"}
    if not args.create:
        print(json.dumps({"mode": "plan", "local_refs": len(local), "remote_refs": len(remote),
                          "worktrees": len(rows), "dirty": sum(bool(r["status"]) for r in rows),
                          "excluded_untracked": sum(len(r["excluded_untracked"]) for r in rows),
                          "missing": len(missing)}, sort_keys=True))
        return
    if missing:
        raise RuntimeError(f"{len(missing)} source tips cannot be peeled to local commits; fetch them before --create")
    tree = archive_tree(repo, manifest, Path(__file__))
    archived = commit(repo, tree, parents, f"Preserve source history {stamp}\n")
    for sha in tips:
        if git(repo, "merge-base", "--is-ancestor", sha, archived, check=False).returncode:
            raise RuntimeError(f"archive does not contain source tip {sha}")
    after_local, after_remote = refs(repo)
    after = {str(path): worktree_fingerprint(path) for path, _ in worktrees(repo)}
    stable = before == after and local == after_local and remote == after_remote
    git(repo, "update-ref", ARCHIVE, archived, local_old or "0" * 40)
    receipt = {"archive_ref": ARCHIVE, "archive_commit": archived, "archive_tree": tree,
               "prior_archive": old, "manifest": manifest, "verified_tips": len(tips),
               "pushed": False, "stable_source_snapshot": stable,
               "changed_worktrees": sorted(path for path in set(before) | set(after) if before.get(path) != after.get(path)),
               "refs_changed_during_capture": local != after_local or remote != after_remote}
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    if args.push and not stable:
        raise RuntimeError(f"source refs or worktrees changed during archive; local receipt at {args.receipt}, no push")
    if args.push:
        git(repo, "push", "origin", ARCHIVE + ":" + ARCHIVE)
        actual = git(repo, "ls-remote", "origin", ARCHIVE).stdout.split("\t", 1)[0]
        if actual != archived:
            raise RuntimeError("remote archive ref differs after push")
        receipt["pushed"] = True
        args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"archive_commit": archived, "archive_tree": tree,
                      "verified_tips": len(tips), "receipt": str(args.receipt),
                      "pushed": receipt["pushed"], "stable_source_snapshot": stable}, sort_keys=True))


if __name__ == "__main__":
    main()

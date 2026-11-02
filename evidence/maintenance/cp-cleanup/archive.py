#!/usr/bin/env python3
"""Read-only ref audit and recoverable archive; never deletes repository refs."""
import collections
import gzip
import hashlib
import json
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
BASE = "81b7ae0c9ce5e7245613ed536b4702c419fe99bd"


def run(args, cwd=ROOT, ok=(0,)):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True)
    if result.returncode not in ok:
        raise RuntimeError(f"{args}: {result.returncode}\n{result.stderr}")
    return result


def git(*args):
    return run(["git", *args]).stdout.strip()


def save(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def main():
    refs = git("for-each-ref", "--format=%(refname)", "refs/heads/research",
               "refs/remotes/origin/research",
               "refs/remotes/origin/task/cp25-reactive-session").splitlines()
    worktrees = git("worktree", "list", "--porcelain")
    entries = []
    for ref in refs:
        oid = git("rev-parse", ref)
        ancestor = run(["git", "merge-base", "--is-ancestor", oid, BASE], ok=(0, 1)).returncode == 0
        commits = git("log", "--format=%H %s", oid, "--not", BASE).splitlines()
        cherry = git("cherry", BASE, oid).splitlines()
        paths = git("diff", "--name-only", BASE + "..." + oid).splitlines()
        entries.append({
            "ref": ref, "oid": oid, "tree": git("rev-parse", oid + "^{tree}"),
            "ancestor_of_baseline": ancestor, "commits_outside_baseline": commits,
            "patch_equivalence": {"command": f"git cherry {BASE} {oid}", "rows": cherry,
                                  "counts": dict(collections.Counter(row[0] for row in cherry))},
            "changed_paths_since_merge_base": paths,
            "contains_other_audited_tips": [other for other in refs if other != ref and
                run(["git", "merge-base", "--is-ancestor", other, oid], ok=(0, 1)).returncode == 0],
            "checked_out": "branch " + ref + "\n" in worktrees,
        })
    attachment = pathlib.Path("/workspace/attachments/zil-cp-cleanup-245")
    attachment.mkdir(parents=True, exist_ok=True)
    bundle = attachment / "research-heads.bundle"
    remote_refs = [entry["ref"] for entry in entries if entry["ref"].startswith("refs/remotes/")]
    if bundle.exists():
        raise RuntimeError("Archive already exists; do not replace the pre-cleanup snapshot")
    run(["git", "bundle", "create", str(bundle), *remote_refs, "^" + BASE])
    verification = run(["git", "bundle", "verify", str(bundle)])
    audit = {
        "baseline": BASE, "main_at_audit": git("rev-parse", "main"),
        "remote_url": git("remote", "get-url", "origin"),
        "shallow": git("rev-parse", "--is-shallow-repository"),
        "worktrees_before": worktrees, "refs": entries,
        "bundle": {"path": bundle.name, "bytes": bundle.stat().st_size,
                   "sha256": hashlib.sha256(bundle.read_bytes()).hexdigest(),
                   "prerequisite_source": BASE,
                   "verify_stdout": verification.stdout, "verify_stderr": verification.stderr},
    }
    detail = json.dumps(audit, ensure_ascii=False, separators=(",", ":")).encode()
    (OUT / "audit-detail.json.gz").write_bytes(gzip.compress(detail, mtime=0))
    for entry in audit["refs"]:
        entry["commits_outside_baseline_count"] = len(entry.pop("commits_outside_baseline"))
        entry["changed_paths_since_merge_base_count"] = len(entry.pop("changed_paths_since_merge_base"))
        entry["patch_equivalence"].pop("rows")
    audit["bundle"].pop("verify_stdout")
    audit["bundle"].pop("verify_stderr")
    audit["bundle"]["extra_attachment"] = str(bundle)
    audit["detail"] = "audit-detail.json.gz"
    save("audit.json", audit)
    # Restore in an independent object database with only the baseline and its ancestry.
    # No alternates or hardlinks; this catches incomplete prerequisite/pack assumptions.
    with tempfile.TemporaryDirectory(prefix="zil245-restore-") as temporary:
        bare = pathlib.Path(temporary) / "restore.git"
        run(["git", "init", "--bare", str(bare)])
        run(["git", "fetch", "--no-tags", str(ROOT), BASE + ":refs/heads/baseline"], cwd=bare)
        restored = run(["git", "bundle", "unbundle", str(bundle)], cwd=bare)
        rows = []
        for entry in entries:
            if not entry["ref"].startswith("refs/remotes/"):
                continue
            name = entry["ref"].removeprefix("refs/remotes/origin/")
            run(["git", "update-ref", "refs/heads/" + name, entry["oid"]], cwd=bare)
            actual = run(["git", "rev-parse", entry["oid"] + "^{tree}"], cwd=bare).stdout.strip()
            assert actual == entry["tree"], entry
            # Check every tree/blob object reachable from the archived head.
            listed = run(["git", "rev-list", "--objects", entry["oid"]], cwd=bare).stdout
            checked = subprocess.run(["git", "cat-file", "--batch-check"], cwd=bare,
                                     input="\n".join(line.split(" ")[0] for line in listed.splitlines()) + "\n",
                                     capture_output=True, text=True, check=True)
            assert " missing" not in checked.stdout
            rows.append({"ref": entry["ref"], "oid": entry["oid"], "tree": actual,
                         "reachable_objects_checked": len(checked.stdout.splitlines()), "status": "Pass"})
        fsck = run(["git", "fsck", "--full", "--no-reflogs"], cwd=bare)
        assert not (bare / "objects/info/alternates").exists()
        save("restore.json", {"status": "Pass", "independent_object_database": True,
             "baseline_fetched_locally": BASE, "bundle_unbundle": restored.stdout,
             "fsck_exit": fsck.returncode, "fsck_stdout": fsck.stdout, "fsck_stderr": fsck.stderr,
             "restored_heads": rows})
    print(json.dumps({"audited_refs": len(entries), "archived_heads": len(remote_refs),
                      "archive_bytes": bundle.stat().st_size, "restore": "Pass"}))


if __name__ == "__main__":
    main()

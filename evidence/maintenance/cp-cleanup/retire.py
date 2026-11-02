#!/usr/bin/env python3
"""One-shot, exact-OID cleanup after durable remote archive verification."""
import json
import pathlib
import subprocess

OUT = pathlib.Path(__file__).resolve().parent
ROOT = OUT.parents[2]
audit = json.loads((OUT / "audit.json").read_text())
tag = json.loads((OUT / "archive-tag.json").read_text())
verified = json.loads((OUT / "remote-restore.json").read_text())
assert verified["status"] == "Pass" and verified["fsck_exit"] == 0
assert verified["archive_commit"] == tag["archive_commit"]
KEEP = "refs/heads/task/cp25-reactive-session"


def run(*args):
    return subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True, check=True)


def remote():
    return {ref: oid for oid, ref in (line.split() for line in
        run("ls-remote", "origin", "refs/heads/*", "refs/tags/" + tag["tag"]).stdout.splitlines())}


before = remote()
assert before["refs/tags/" + tag["tag"]] == tag["tag_oid"]
deletions = []
for entry in audit["refs"]:
    if not entry["ref"].startswith("refs/remotes/origin/"):
        continue
    ref = entry["ref"].replace("refs/remotes/origin/", "refs/heads/")
    assert before[ref] == entry["oid"], f"Remote changed: {ref}; reaudit required"
    if ref != KEEP:
        deletions.append({"remote_ref": ref, "tracking_ref": entry["ref"], "oid": entry["oid"]})
assert len(deletions) == 13
cmd = ["push", "--atomic"]
cmd += ["--force-with-lease=" + row["remote_ref"] + ":" + row["oid"] for row in deletions]
cmd += ["origin"] + [":" + row["remote_ref"] for row in deletions]
(OUT / "retirement.json").write_text(json.dumps({"status": "Prepared",
    "remote_before": before, "atomic_push_command": ["git", *cmd],
    "deleted_remote": deletions}, indent=2) + "\n")
pushed = run(*cmd)
after = remote()
for row in deletions:
    assert row["remote_ref"] not in after
assert after[KEEP] == before[KEEP]
assert after["refs/tags/" + tag["tag"]] == tag["tag_oid"]
unrelated_changes = {ref: {"before": oid, "after": after.get(ref)}
    for ref, oid in before.items()
    if ref not in {row["remote_ref"] for row in deletions} and after.get(ref) != oid}
local = [entry for entry in audit["refs"] if entry["ref"].startswith("refs/heads/")]
for entry in local:
    assert entry["ancestor_of_baseline"] and not entry["checked_out"]
    assert "branch " + entry["ref"] + "\n" not in run("worktree", "list", "--porcelain").stdout
    run("merge-base", "--is-ancestor", entry["oid"], audit["baseline"])
    run("update-ref", "-d", entry["ref"], entry["oid"])
for row in deletions:
    current = subprocess.run(["git", "show-ref", "--verify", "--hash", row["tracking_ref"]],
                             cwd=ROOT, text=True, capture_output=True)
    if current.returncode == 0:
        assert current.stdout.strip() == row["oid"]
        run("update-ref", "-d", row["tracking_ref"], row["oid"])
result = {"status": "Pass", "remote_before": before, "remote_after": after,
          "atomic_push_command": ["git", *cmd], "push_stdout": pushed.stdout,
          "push_stderr": pushed.stderr, "deleted_remote": deletions,
          "deleted_local": local, "kept": {KEEP: after[KEEP]},
          "unrelated_remote_changes_during_cleanup": unrelated_changes,
          "archive_tag": tag["tag"], "archive_commit": tag["archive_commit"]}
(OUT / "retirement.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"status": "Pass", "deleted_remote": len(deletions), "deleted_local": len(local)}))

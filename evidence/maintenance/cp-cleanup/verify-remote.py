#!/usr/bin/env python3
"""Verify a fresh, full remote tag fetch without alternates or a local object source."""
import json
import pathlib
import subprocess
import sys

out = pathlib.Path(__file__).resolve().parent
bare = pathlib.Path(sys.argv[1]).resolve()
audit = json.loads((out / "audit.json").read_text())
tag = json.loads((out / "archive-tag.json").read_text())


def git(*args):
    return subprocess.check_output(["git", "-C", str(bare), *args], text=True).strip()


assert not (bare / "objects/info/alternates").exists()
assert git("rev-parse", "--is-shallow-repository") == "false"
assert git("rev-parse", "refs/tags/" + tag["tag"]) == tag["tag_oid"]
assert git("rev-parse", "refs/tags/" + tag["tag"] + "^{}") == tag["archive_commit"]
assert git("rev-parse", tag["archive_commit"] + "^{tree}") == tag["tree"]
parents = git("show", "-s", "--format=%P", tag["archive_commit"]).split()
assert parents == tag["parents"]
heads = []
for entry in audit["refs"]:
    if not entry["ref"].startswith("refs/remotes/"):
        continue
    assert entry["oid"] in parents
    actual = git("rev-parse", entry["oid"] + "^{tree}")
    assert actual == entry["tree"]
    heads.append({"ref": entry["ref"], "oid": entry["oid"], "tree": actual, "status": "Pass"})
fsck = subprocess.run(["git", "-C", str(bare), "fsck", "--full", "--no-reflogs"],
                      text=True, capture_output=True)
assert fsck.returncode == 0, fsck.stderr
result = {"status": "Pass", "source": "https://github.com/zil-lang/zil.git",
          "fetch_command": "git fetch --no-tags https://github.com/zil-lang/zil.git "
          "refs/tags/" + tag["tag"] + ":refs/tags/" + tag["tag"],
          "tag_oid": tag["tag_oid"], "archive_commit": tag["archive_commit"],
          "full_independent_remote_fetch": True, "alternates": False, "shallow": False,
          "fsck_exit": fsck.returncode, "fsck_stdout": fsck.stdout, "fsck_stderr": fsck.stderr,
          "heads": heads}
(out / "remote-restore.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"status": "Pass", "remote_restored_heads": len(heads), "fsck_exit": fsck.returncode}))

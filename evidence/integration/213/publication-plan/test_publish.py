#!/usr/bin/env python3
"""Offline ref-protocol tests. Synthetic gate receipts never claim language verification."""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "publish.py"
TEMPLATE = HERE / "manifest.template.json"

def cmd(args, cwd, *, ok=True, data=None):
    result = subprocess.run(args, cwd=cwd, input=data, text=True, capture_output=True)
    if ok and result.returncode:
        raise AssertionError(f"{args}\n{result.stdout}\n{result.stderr}")
    return result

def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

class Fixture:
    def __init__(self, root):
        self.root = Path(root)
        self.origin = self.root / "remote.git"
        self.author = self.root / "author"
        self.repo = self.root / "runner"
        self.payload = self.root / "payload"
        self.payload.mkdir()
        self.receipts = self.root / "receipts"
        self.receipts.mkdir()
        cmd(["git", "init", "--bare", "--initial-branch=main", str(self.origin)], self.root)
        cmd(["git", "init", "--initial-branch=main", str(self.author)], self.root)
        self.git = lambda *args, **kw: cmd(["git", *args], self.author, **kw)
        self.git("config", "user.name", "Offline publication test")
        self.git("config", "user.email", "offline@example.invalid")
        self.tree = self.git("mktree", data="").stdout.strip()
        self.base = self.commit("main")
        self.merged = [self.commit("merged" + str(i), self.base) for i in range(3)]
        self.isolated = [self.commit("archive-only" + str(i), self.base) for i in range(3)]
        self.source = self.commit("verified-source", self.base, *self.merged)
        self.transport = self.commit("transport-with-payload", self.base)
        self.drift = self.commit("concurrent-work", self.base)
        self.git("update-ref", "refs/heads/main", self.base)
        self.git("update-ref", "refs/heads/task/213", self.source)
        self.git("remote", "add", "origin", str(self.origin))
        self.manifest = json.loads(TEMPLATE.read_text())
        self.manifest["repository"] = "local/zil"
        self.manifest["main"]["expected_sha"] = self.base
        next_merged = iter(self.merged)
        next_archive = iter(self.isolated)
        self.heads = {"refs/heads/main": self.base, "refs/heads/task/213": self.transport}
        for item in self.manifest["retire"]:
            item["expected_sha"] = next(next_merged if item["disposition"] == "merged" else next_archive)
            self.heads[item["ref"]] = item["expected_sha"]
        self.git("push", "origin", *[sha + ":" + ref for ref, sha in self.heads.items()], self.drift + ":refs/tags/test-drift-object")
        self.bundle = self.payload / "source.bundle"
        self.make_bundle()
        self.path = self.payload / "manifest.json"
        self.save_manifest()
        cmd(["git", "clone", str(self.origin), str(self.repo)], self.root)

    def commit(self, message, *parents):
        args = ["commit-tree", self.tree]
        for parent in parents:
            args += ["-p", parent]
        return self.git(*args, data=message + "\n").stdout.strip()

    def make_bundle(self):
        self.git("update-ref", "refs/heads/task/213", self.source)
        self.git("bundle", "create", str(self.bundle), "refs/heads/task/213", "^" + self.base)
        self.manifest["source"]["sha"] = self.source
        self.manifest["source"]["bundle_sha256"] = sha256(self.bundle)

    def save_manifest(self):
        self.path.write_text(json.dumps(self.manifest, indent=2) + "\n")

    def refs(self):
        rows = cmd(["git", "for-each-ref", "--format=%(objectname) %(refname)"], self.origin).stdout.splitlines()
        return {row.split(" ", 1)[1]: row.split(" ", 1)[0] for row in rows}

    def remote_update(self, ref, sha):
        cmd(["git", "update-ref", ref, sha], self.origin)

    def call(self, mode, ok=True):
        argv = [sys.executable, str(SCRIPT), mode, "--repo", str(self.repo),
                "--manifest", str(self.path), "--bundle", str(self.bundle),
                "--repository", "local/zil", "--transport-sha", self.transport,
                "--receipt", str(self.receipts / "prepared.json")]
        if mode == "publish":
            argv += ["--gates", str(self.receipts / "gates.json"), "--result", str(self.receipts / "published.json")]
        return cmd(argv, self.root, ok=ok)

    def prepare(self):
        self.call("prepare")
        # This suite tests Git state transitions only, not ZIL compilation.
        from publish import GATE_IDS
        receipt = json.loads((self.receipts / "prepared.json").read_text())
        receipt["stage"] = "verified"
        receipt["status"] = "Pass"
        receipt["gates"] = []
        receipt["test_fixture"] = "SYNTHETIC gate records for isolated Git protocol tests only"
        for gate in GATE_IDS:
            log = self.receipts / (gate + ".log")
            log.write_text("SYNTHETIC offline Git protocol fixture; ZIL gate not executed.\n")
            receipt["gates"].append({"id": gate, "exit_code": 0, "log": log.name, "log_sha256": sha256(log)})
        (self.receipts / "gates.json").write_text(json.dumps(receipt))

    def hook(self, name, contents):
        path = self.origin / "hooks" / name
        path.write_text("#!/bin/sh\nset -eu\n" + contents)
        path.chmod(0o755)

class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="publish213-", dir=HERE)
        self.f = Fixture(self.tmp.name)
    def tearDown(self):
        self.tmp.cleanup()
    def refused(self, result, text):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(text, result.stderr)
    def assert_original_heads(self):
        actual = {r:s for r,s in self.f.refs().items() if r.startswith("refs/heads/")}
        self.assertEqual(actual, self.f.heads)

    def test_dry_run_and_prepare_do_not_write_remote(self):
        before = self.f.refs()
        report = json.loads(self.f.call("dry-run").stdout)
        self.assertFalse(report["remote_writes"])
        self.assertEqual(before, self.f.refs())
        self.f.prepare()
        self.assertEqual(before, self.f.refs())
        self.assertEqual(cmd(["git", "rev-parse", "HEAD"], self.f.repo).stdout.strip(), self.f.source)

    def test_fast_forward_archives_and_atomic_retirement(self):
        self.f.prepare()
        self.f.call("publish")
        refs = self.f.refs()
        self.assertEqual({r:s for r,s in refs.items() if r.startswith("refs/heads/")}, {"refs/heads/main": self.f.source})
        for item in [self.f.manifest["main"], *self.f.manifest["retire"]]:
            self.assertEqual(refs[item["archive_tag"]], item["expected_sha"])
        self.assertEqual(refs[self.f.manifest["transport"]["archive_tag"]], self.f.transport)
        guard = refs[self.f.manifest["guard_tag"]]
        parents = cmd(["git", "show", "-s", "--format=%P", guard], self.f.origin).stdout.split()
        self.assertEqual(set(parents), set(self.f.heads.values()) | {self.f.source})
        self.assertEqual(cmd(["git", "merge-base", "--is-ancestor", guard, self.f.source], self.f.origin, ok=False).returncode, 1)

    def test_existing_exact_archive_tag_is_preserved(self):
        tag = self.f.manifest["main"]["archive_tag"]
        self.f.remote_update(tag, self.f.base)
        self.f.prepare()
        self.f.call("publish")
        self.assertEqual(self.f.refs()[tag], self.f.base)

    def test_main_drift_stops_before_archiving(self):
        self.f.prepare()
        self.f.remote_update("refs/heads/main", self.f.drift)
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "remote heads drifted")
        self.assertEqual(before, self.f.refs())

    def test_retiring_branch_drift_is_not_lost(self):
        self.f.prepare()
        ref = self.f.manifest["retire"][0]["ref"]
        self.f.remote_update(ref, self.f.drift)
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "remote heads drifted")
        self.assertEqual(before, self.f.refs())

    def test_new_branch_stops_before_archiving(self):
        self.f.prepare()
        self.f.remote_update("refs/heads/new-work", self.f.drift)
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "remote heads drifted")
        self.assertEqual(before, self.f.refs())

    def test_tag_drift_is_never_overwritten(self):
        self.f.prepare()
        self.f.remote_update(self.f.manifest["main"]["archive_tag"], self.f.drift)
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "archive tag drifted")
        self.assertEqual(before, self.f.refs())

    def test_missing_or_failed_gate_refuses_all_writes(self):
        self.f.prepare()
        path = self.f.receipts / "gates.json"
        receipt = json.loads(path.read_text())
        receipt["gates"][1]["exit_code"] = 1
        path.write_text(json.dumps(receipt))
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "verification gate failed")
        self.assertEqual(before, self.f.refs())

    def test_changed_gate_log_refuses_all_writes(self):
        self.f.prepare()
        (self.f.receipts / "workspace.log").write_text("modified")
        before = self.f.refs()
        self.refused(self.f.call("publish", ok=False), "gate log checksum mismatch")
        self.assertEqual(before, self.f.refs())

    def test_checksum_mismatch_refuses_prepare(self):
        self.f.bundle.write_bytes(self.f.bundle.read_bytes() + b"tampered")
        before = self.f.refs()
        self.refused(self.f.call("prepare", ok=False), "bundle SHA256 mismatch")
        self.assertEqual(before, self.f.refs())

    def test_non_fast_forward_source_is_rejected(self):
        self.f.source = self.f.commit("unrelated-root")
        self.f.make_bundle()
        self.f.save_manifest()
        before = self.f.refs()
        self.refused(self.f.call("prepare", ok=False), "main would not be fast-forward")
        self.assertEqual(before, self.f.refs())

    def test_nogo_research_ancestor_is_rejected(self):
        self.f.source = self.f.commit("bad-merge", self.f.source, *self.f.isolated)
        self.f.make_bundle()
        self.f.save_manifest()
        before = self.f.refs()
        self.refused(self.f.call("prepare", ok=False), "archive-only research/work was merged")
        self.assertEqual(before, self.f.refs())

    def test_branch_protection_rejection_keeps_main_and_all_heads(self):
        self.f.prepare()
        protected = self.f.manifest["retire"][-1]["ref"]
        self.f.hook("update", 'if [ "$1" = "' + protected + '" ] && [ "$3" = "0000000000000000000000000000000000000000" ]; then\n  echo "protected branch deletion refused" >&2\n  exit 1\nfi\n')
        self.refused(self.f.call("publish", ok=False), "failed")
        self.assert_original_heads()
        refs = self.f.refs()
        self.assertNotIn(self.f.manifest["guard_tag"], refs)
        for item in [self.f.manifest["main"], *self.f.manifest["retire"]]:
            self.assertEqual(refs[item["archive_tag"]], item["expected_sha"])

    def test_head_drift_after_archive_creation_aborts_final_transaction(self):
        self.f.prepare()
        ref = self.f.manifest["retire"][0]["ref"]
        self.f.hook("post-receive", 'while read old new ref; do\n case "$ref" in refs/tags/archive/*) git update-ref "' + ref + '" "' + self.f.drift + '";; esac\ndone\n')
        self.refused(self.f.call("publish", ok=False), "remote heads drifted")
        self.assertEqual(self.f.refs()["refs/heads/main"], self.f.base)
        self.assertEqual(self.f.refs()[ref], self.f.drift)
        self.assertEqual(len([r for r in self.f.refs() if r.startswith("refs/heads/")]), len(self.f.heads))

    def test_remote_compare_and_swap_detects_last_moment_race(self):
        self.f.prepare()
        ref = self.f.manifest["retire"][0]["ref"]
        guard = self.f.manifest["guard_tag"]
        # Concurrent update occurs after push advertisement, inside receive-pack.
        # The actual expected-old ref transaction must reject it atomically.
        self.f.hook("pre-receive", 'while read old new ref; do\n if [ "$ref" = "' + guard + '" ]; then\n  env -u GIT_QUARANTINE_PATH git update-ref "' + ref + '" "' + self.f.drift + '"\n fi\ndone\n')
        self.refused(self.f.call("publish", ok=False), "failed")
        refs = self.f.refs()
        self.assertEqual(refs["refs/heads/main"], self.f.base)
        self.assertEqual(refs[ref], self.f.drift)
        self.assertNotIn(guard, refs)
        self.assertEqual(len([r for r in refs if r.startswith("refs/heads/")]), len(self.f.heads))

if __name__ == "__main__":
    unittest.main(verbosity=2)

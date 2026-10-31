"""Safety controls use a fake remote; these tests never access GitHub."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import cleanup as c

MANIFEST = json.loads(Path(__file__).with_name("manifest.json").read_text())
TASK_SHA = "a" * 40
MAIN_SHA = "b" * 40
ENV = {"GITHUB_REPOSITORY": c.REPO, "GITHUB_REF": c.TASK, "GITHUB_SHA": TASK_SHA,
       "GITHUB_EVENT_NAME": "push"}


class FakeAudit(c.Audit):
    def __init__(self, out, execute=False):
        super().__init__(out, "execute" if execute else "dry-run", "manifest", "script")
        self.state = {c.MAIN: MAIN_SHA, c.TASK: TASK_SHA,
                      "refs/heads/research/incremental-compilation": "c" * 40,
                      "refs/heads/task/cp25-reactive-session": "d" * 40,
                      **{r["ref"]: r["expected_sha"] for r in MANIFEST["branches"]}}
        self.commands = []
        self.fail = set()
        self.hooks = {}
        self.paths = ["docs/LANGUAGE-CONSTRUCTION11.md", ".task/impl/232.md"]
        self.ci_conclusion = "success"

    def refs(self, name):
        if name in self.hooks:
            self.hooks[name](self)
        self.record(name, self.state.copy())
        return self.state.copy()

    def api(self, name, suffix):
        return {"id": 41198113948, "run_attempt": 1, "status": "completed",
                "conclusion": self.ci_conclusion, "event": "push", "head_branch": "main",
                "head_sha": MANIFEST["accepted_head"], "path": ".github/workflows/verify-main.yml",
                "repository": {"full_name": c.REPO}, "head_repository": {"full_name": c.REPO}}

    def run(self, name, args, **kwargs):
        self.commands.append((name, args))
        self.record(name, args)
        if name in self.fail:
            raise c.Refusal("simulated failure " + name)
        if name == "resolved-remote-url":
            return c.REMOTE.encode(), 0
        if name.startswith("fetched-tip-"):
            i = int(name.rsplit("-", 1)[1])
            refs = [c.MAIN, *(r["ref"] for r in MANIFEST["branches"]), c.TASK]
            return self.state[refs[i]].encode(), 0
        if name == "main-metadata-delta":
            return ("\0".join(self.paths) + "\0").encode(), 0
        if name.startswith("bounded-target-paths-"):
            i = int(name.rsplit("-", 1)[1])
            path = "ops-audit/cleanup.py" if i == len(MANIFEST["branches"]) else MANIFEST["branches"][i]["allowed_files"][-1]
            return path.encode() + b"\0", 0
        if name.startswith("transport-manifest-"):
            i = int(name.rsplit("-", 1)[1])
            return c.json_bytes({"targets": [{"sha": s} for s in MANIFEST["branches"][i]["published_commits"]]}), 0
        if name in {"create-only-archives", "atomic-leased-deletion"}:
            self.assert_push_shape(args)
            for spec in args[args.index(c.REMOTE) + 1:]:
                sha, ref = spec.split(":", 1)
                if sha:
                    self.state[ref] = sha
                else:
                    self.state.pop(ref)
        return b"", 0

    @staticmethod
    def assert_push_shape(args):
        assert "--atomic" in args and "--porcelain" in args
        for spec in args[args.index(c.REMOTE) + 1:]:
            sha, ref = spec.split(":", 1)
            assert ref != c.MAIN and not ref.startswith("refs/heads/research/")
            assert ref != "refs/heads/task/cp25-reactive-session"
            if sha:
                assert ref.startswith("refs/tags/archive/20261031/")
                assert f"--force-with-lease={ref}:" in args
            else:
                assert any(x.startswith(f"--force-with-lease={ref}:") for x in args)


class SafetyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.env = patch.dict(os.environ, ENV)
        self.env.start()
        self.records = patch.object(c, "check_records", return_value={"new_errors": [], "new_warnings": []})
        self.records.start()

    def tearDown(self):
        self.records.stop()
        self.env.stop()
        self.temp.cleanup()

    def audit(self, name="dry", execute=False):
        return FakeAudit(self.root / name, execute)

    def dry(self, a=None):
        a = a or self.audit()
        c.perform(a, MANIFEST, execute=False, dry_path=None)
        a.save()
        return a

    def execute(self, dry, a=None):
        a = a or self.audit("execute", True)
        c.perform(a, MANIFEST, execute=True, dry_path=dry.out / "receipt.json")
        return a

    def test_fixed_manifest_rejects_research_or_new_tip(self):
        c.validate_manifest(MANIFEST)
        for field, value in [("ref", "refs/heads/research/compile-performance"), ("expected_sha", "e" * 40)]:
            m = copy.deepcopy(MANIFEST)
            m["branches"][0][field] = value
            with self.assertRaises(c.Refusal):
                c.validate_manifest(m)

    def test_chinese_paths_are_literal_nul_records(self):
        p = "docs/log/zil施工8.md"
        self.assertEqual(c.nul_paths(p.encode() + b"\0"), [p])
        self.assertTrue(c.matches(p, MANIFEST["branches"][0]["allowed_files"], []))
        with self.assertRaises(c.Refusal):
            c.nul_paths(p.encode())

    def test_dry_run_has_no_remote_mutation(self):
        a = self.dry()
        self.assertEqual(a.value["status"], "DryRunValidated")
        self.assertFalse(a.value["mutation_attempted"])
        self.assertFalse(any(name in {"create-only-archives", "atomic-leased-deletion"} for name, _ in a.commands))

    def test_target_movement_refuses_before_mutation(self):
        a = self.audit()
        a.state[MANIFEST["branches"][0]["ref"]] = "e" * 40
        with self.assertRaises(c.Refusal):
            self.dry(a)
        self.assertFalse(a.value["mutation_attempted"])

    def test_archive_collision_is_never_overwritten(self):
        a = self.audit()
        a.state[MANIFEST["branches"][0]["archive_tag"]] = "e" * 40
        with self.assertRaises(c.Refusal):
            self.dry(a)
        self.assertFalse(a.value["mutation_attempted"])

    def test_failed_ci_or_untested_production_blocks_cleanup(self):
        for name in ["ci", "code"]:
            a = self.audit(name)
            if name == "ci":
                a.ci_conclusion = "failure"
            else:
                a.paths.append("crates/zil-types/src/lib.rs")
            with self.assertRaises(c.Refusal):
                self.dry(a)
            self.assertFalse(a.value["mutation_attempted"])

    def test_unmerged_published_object_blocks_cleanup(self):
        a = self.audit()
        a.fail.add("published-target-in-main-4-0")
        with self.assertRaises(c.Refusal):
            self.dry(a)
        self.assertFalse(a.value["mutation_attempted"])

    def test_execute_requires_same_main_and_receipt(self):
        dry = self.dry()
        a = self.audit("execute", True)
        a.state[c.MAIN] = "e" * 40
        with self.assertRaises(c.Refusal):
            self.execute(dry, a)
        self.assertFalse(a.value["mutation_attempted"])

    def test_success_archives_eight_exact_tips_and_keeps_other_refs(self):
        dry = self.dry()
        a = self.execute(dry)
        self.assertEqual(a.value["status"], "ArchivedDeletedAndVerified")
        self.assertEqual(len(a.value["targets"]), 8)
        self.assertEqual(a.value["unrelated_ref_changes"], [])
        self.assertIn(c.MAIN, a.state)
        self.assertIn("refs/heads/research/incremental-compilation", a.state)
        self.assertIn("refs/heads/task/cp25-reactive-session", a.state)
        self.assertEqual(sum(name == "atomic-leased-deletion" for name, _ in a.commands), 1)
        for row in a.value["targets"]:
            self.assertNotIn(row["ref"], a.state)
            self.assertEqual(a.state[row["archive_tag"]], row["expected_sha"])

    def test_target_moves_after_archive_prevents_all_deletion(self):
        dry = self.dry()
        a = self.audit("execute", True)
        a.hooks["immediate-pre-delete"] = lambda x: x.state.update({MANIFEST["branches"][0]["ref"]: "e" * 40})
        with self.assertRaises(c.Refusal):
            self.execute(dry, a)
        self.assertFalse(any(name == "atomic-leased-deletion" for name, _ in a.commands))
        self.assertIn(c.TASK, a.state)

    def test_uncertain_delete_is_not_automatically_retried(self):
        dry = self.dry()
        a = self.audit("execute", True)
        a.fail.add("atomic-leased-deletion")
        with self.assertRaises(c.Refusal):
            self.execute(dry, a)
        self.assertEqual(sum(name == "atomic-leased-deletion" for name, _ in a.commands), 1)

    def test_new_handoff_error_preserves_failure_and_blocks_mutation(self):
        a = self.audit()
        with patch.object(c, "check_records", side_effect=c.Refusal("New handoff issues")):
            with self.assertRaises(c.Refusal):
                self.dry(a)
        self.assertFalse(a.value["mutation_attempted"])


class RecordGateTests(unittest.TestCase):
    """Exercise check_records itself; only external command execution is replaced."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.old = {"id": "010", "reason": "Historical object missing"}
        self.reports = {}
        for label, sha in [("baseline", MANIFEST["accepted_head"]), ("candidate", MAIN_SHA)]:
            self.reports[label] = {
                "main_head": sha, "current_head": sha, "authority": {"current_is_main": True},
                "status": "Fail", "errors": [self.old.copy()], "warnings": [],
                "slices": [{"id": n, "status": "已合入", "committed": True} for n in ("232", "233", "234")],
            }

    def tearDown(self):
        self.temp.cleanup()

    def run_gate(self):
        a = c.Audit(self.root / "receipt", "dry-run", "manifest", "script")
        reports = self.reports
        def command(name, args, **kwargs):
            if name.endswith("-handoff"):
                value = reports[name.split("-", 1)[0]]
                return c.json_bytes(value), 1 if value["errors"] else 0
            if name.endswith("-plan-self-test"):
                return c.json_bytes({"ok": True, "tasks": 14, "self_tests": [{"rejected": True}] * 7}), 0
            return b"", 0
        with patch.object(a, "run", side_effect=command):
            return c.check_records(a, self.root / "objects.git", MANIFEST, MAIN_SHA, self.root)

    def test_inherited_failure_stays_failure_and_three_cards_are_zero(self):
        result = self.run_gate()
        self.assertEqual(result["candidate_status"], "Fail")
        self.assertEqual(result["candidate_error_count"], 1)
        self.assertEqual(result["new_errors"], [])
        self.assertEqual(set(result["candidate_cards"]), {"232", "233", "234"})
        self.assertTrue(all(v["errors"] == 0 and v["warnings"] == 0 for v in result["candidate_cards"].values()))

    def test_existing_232_error_is_rejected_even_without_any_new_error(self):
        error = {"id": "232", "reason": "Existing card defect"}
        for report in self.reports.values():
            report["errors"].append(error)
        with self.assertRaisesRegex(c.Refusal, "232/233/234"):
            self.run_gate()
        receipt = json.loads((self.root / "receipt/receipt.json").read_text())
        delta = next(x["result"] for x in receipt["steps"] if x["name"] == "record-integrity-delta")
        self.assertEqual(delta["new_errors"], [])
        self.assertEqual(delta["candidate_cards"]["232"]["errors"], 1)

    def test_card_path_error_and_unmerged_record_are_rejected(self):
        for report in self.reports.values():
            report["errors"].append({"id": ".task/impl/233.md", "reason": "Malformed card"})
        self.reports["candidate"]["slices"][1]["status"] = "待验"
        with self.assertRaisesRegex(c.Refusal, "232/233/234"):
            self.run_gate()

    def test_new_error_and_warning_cannot_be_hidden_by_old_count(self):
        self.reports["candidate"]["errors"] = [{"id": "011", "reason": "Different defect, same count"}]
        self.reports["candidate"]["warnings"].append({"id": "old", "reason": "New warning"})
        with self.assertRaisesRegex(c.Refusal, "New handoff issues"):
            self.run_gate()


if __name__ == "__main__":
    unittest.main()

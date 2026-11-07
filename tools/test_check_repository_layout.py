"""Integration checks for incremental and full repository layout gates."""

import importlib.util
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "repository_layout", Path(__file__).with_name("check-repository-layout.py")
)
layout = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(layout)


class RepositoryLayoutTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Layout Test")
        self.git("config", "user.email", "layout@example.invalid")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout

    def write(self, path):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("fixture\n", encoding="utf-8")

    def baseline(self, *paths):
        for path in paths:
            self.write(path)
        self.git("add", "-A")
        self.git("commit", "-qm", "baseline")
        return self.git("rev-parse", "HEAD").decode().strip()

    def test_new_numbered_paths_and_renames_fail(self):
        base = self.baseline("docs/guide.md")
        self.write("crates/G10.7-engine/src/lib.rs")
        self.write("tests/impl253_verify.rs")
        self.write("examples/123/main.zil")
        self.git("mv", "docs/guide.md", "docs/G2.3-guide.md")
        _, failures, _ = layout.check(base, root=self.root)
        self.assertEqual({path for path, _ in failures}, {
            "crates/G10.7-engine/src/lib.rs", "tests/impl253_verify.rs",
            "examples/123/main.zil", "docs/G2.3-guide.md",
        })

    def test_existing_numbered_paths_are_audited_without_failing(self):
        base = self.baseline("tools/language/203/consumer.py", "changes/G1.3.2-record.md")
        new, failures, legacy = layout.check(base, root=self.root)
        self.assertEqual(new, [])
        self.assertEqual(failures, [])
        self.assertEqual({path for path, _ in legacy}, {
            "tools/language/203/consumer.py", "changes/G1.3.2-record.md",
        })

    def test_versions_and_stable_cases_survive(self):
        base = self.baseline("README.md")
        for path in ("tools/llvm23/check.py", "examples/wire-v40/main.zil",
                     "tests/case123_u64.rs", "tests/case_344.rs",
                     "changes/schema-v40.md", "changes/sha-abcdef1234.md",
                     "changes/SOURCE-SNAPSHOT-20261026.md"):
            self.write(path)
        _, failures, _ = layout.check(base, root=self.root)
        self.assertEqual(failures, [])

    def test_new_construction_symbol_fails_but_existing_symbol_is_legacy(self):
        self.write("crates/core/src/lib.rs")
        source = self.root / "crates/core/src/lib.rs"
        source.write_text("fn impl42_old() {}\n", encoding="utf-8")
        base = self.baseline("README.md")
        source.write_text("fn impl42_old() {}\nfn g1_3_new() {}\n", encoding="utf-8")
        _, failures, _ = layout.check(base, root=self.root)
        self.assertEqual(failures, [
            ("crates/core/src/lib.rs", "construction ID declaration g1_3_new")
        ])

    def test_delivery_rejects_task_state_even_when_in_baseline(self):
        base = self.baseline(".task/G10.7.5.1.md", "_zil_goal/current.md", "goal.md",
                             "data/handoff.json", "data/construction-progress.json",
                             "data/task-index.json")
        self.write(".task/G2.3.md")
        _, ordinary_failures, _ = layout.check(base, root=self.root)
        _, delivery_failures, _ = layout.check(base, delivery=True, root=self.root)
        self.assertEqual(ordinary_failures, [])
        self.assertEqual({path for path, _ in delivery_failures}, {
            ".task/G10.7.5.1.md", ".task/G2.3.md",
            "_zil_goal/current.md", "goal.md",
            "data/handoff.json", "data/construction-progress.json", "data/task-index.json",
        })

    def test_other_construction_labels_and_numbered_filenames_fail(self):
        base = self.baseline("README.md")
        paths = ("docs/g1_3-plan.md", "changes/LANGUAGE-219.md",
                 "changes/BATCH336.md", "changes/IMPLEMENTATION-0063.md",
                 "changes/344-feature.md", "tests/feature_344.rs",
                 "tools/123.py")
        for path in paths:
            self.write(path)
        _, failures, _ = layout.check(base, root=self.root)
        self.assertEqual({path for path, _ in failures}, set(paths))

    def test_full_rejects_numbered_paths_already_in_baseline(self):
        self.baseline("crates/impl253/src/lib.rs", "tests/cp34_fingerprint.rs",
                      "tools/batch336/probe.py")
        checked, failures, legacy = layout.check(full=True, root=self.root)
        self.assertEqual(len(checked), 3)
        self.assertEqual(legacy, [])
        self.assertEqual({path for path, _ in failures}, {
            "crates/impl253/src/lib.rs", "tests/cp34_fingerprint.rs",
            "tools/batch336/probe.py",
        })

    def test_full_accepts_protocol_width_case_and_fixture_numbers(self):
        self.baseline("crates/schema-v40/src/wire_v34.rs", "tools/llvm23/check.py",
                      "tests/case_344.rs", "tests/fixture01_valid_callback.rs",
                      "examples/m0/main.zil", "tests/fixtures/01_record.zil",
                      "tests/cases/PLAN-T01-003.json",
                      "tests/perf/chain-reverse-256.zil")
        source = self.root / "crates/schema-v40/src/wire_v34.rs"
        source.write_text(
            "fn schema40_decode() {}\nfn u64_roundtrip() {}\n"
            "fn legal_240_level_value() {}\nfn all_65536_pairs() {}\n"
            "fn buffered_sha256_blocks() {}\n",
            encoding="utf-8",
        )
        _, failures, _ = layout.check(full=True, root=self.root)
        self.assertEqual(failures, [])

    def test_short_case_ordinal_needs_a_name_and_three_digits_fail(self):
        self.baseline("README.md")
        for path in ("tests/fixtures/01_record.zil", "tests/fixtures/1-record.zil",
                     "tests/fixtures/344_record.zil", "tests/fixtures/01.zil",
                     "tests/fixtures/01_2.zil"):
            self.write(path)
        _, failures, _ = layout.check(full=True, root=self.root)
        self.assertEqual({path for path, _ in failures}, {
            "tests/fixtures/344_record.zil", "tests/fixtures/01.zil",
            "tests/fixtures/01_2.zil",
        })

    def test_attached_construction_numbers_in_paths_fail(self):
        self.baseline("README.md")
        paths = (
            "crates/compiler/src/builder210.rs", "crates/compiler/src/allocation211.rs",
            "crates/compiler/src/native211.rs", "crates/compiler/src/machine210.rs",
            "crates/compiler/tests/llvm_api208.rs", "crates/compiler/src/object_cache212.rs",
            "crates/compiler/tests/perf206_codegen.rs", "tests/fixtures/original235.zil",
            "tests/fixtures/02_original201.zil", "tools/run338.py",
            "validation/native339.json", "tests/perf/modules/m512.zil",
            "tests/perf/modules/m001.zil", "tools/sha999.py",
        )
        for path in paths:
            self.write(path)
        _, failures, _ = layout.check(full=True, root=self.root)
        self.assertEqual({path for path, _ in failures}, set(paths))

    def test_attached_semantic_numbers_in_paths_survive(self):
        self.baseline("README.md")
        for path in (
            "crates/compiler/src/sha256_stream.rs", "crates/compiler/src/u128_ops.rs",
            "crates/compiler/src/f128_ops.rs",
            "crates/compiler/src/schema100_wire.rs", "crates/compiler/src/wire_v100.rs",
            "tools/llvm300/check.py", "tests/case344_record.zil",
            "tests/perf/modules/m0.zil", "tests/perf/modules/m511.zil",
            "tests/cases/PLAN-T01-003.json", "tests/perf/chain-reverse-256.zil",
            "tests/fixtures/01_record.zil",
        ):
            self.write(path)
        _, failures, _ = layout.check(full=True, root=self.root)
        self.assertEqual(failures, [])

    def test_full_scans_existing_production_declarations(self):
        self.baseline("crates/compiler/src/lib.rs", "tools/check.py")
        (self.root / "crates/compiler/src/lib.rs").write_text(
            "pub fn impl243_layout() {}\nfn product237_budget() {}\n"
            "fn impl243_all_65536_pairs() {}\n",
            encoding="utf-8",
        )
        (self.root / "tools/check.py").write_text("def batch336_probe():\n    pass\n", encoding="utf-8")
        _, failures, _ = layout.check(full=True, root=self.root)
        self.assertEqual({issue for _, issue in failures}, {
            "construction ID declaration impl243_layout",
            "construction ID declaration product237_budget",
            "construction ID declaration impl243_all_65536_pairs",
            "construction ID declaration batch336_probe",
        })

    def test_full_delivery_rejects_existing_task_state(self):
        self.baseline(".task/G10.7.5.1.md", "docs/guide.md")
        _, failures, _ = layout.check(full=True, delivery=True, root=self.root)
        self.assertIn((".task/G10.7.5.1.md", "task-state directory"), failures)

    def test_full_cli_needs_no_baseline(self):
        with patch.object(layout, "check", return_value=([], [], [])) as check:
            with redirect_stdout(StringIO()):
                self.assertEqual(layout.main(["--full", "--delivery"]), 0)
        check.assert_called_once_with(None, True, full=True)


if __name__ == "__main__":
    unittest.main()

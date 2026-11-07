"""Integration checks for the repository layout gate's Git baseline behavior."""

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path


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
                     "tests/case123_u64.rs", "changes/sha-abcdef1234.md"):
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
        base = self.baseline(".task/active.md", "_zil_goal/current.md", "goal.md")
        _, ordinary_failures, _ = layout.check(base, root=self.root)
        _, delivery_failures, _ = layout.check(base, delivery=True, root=self.root)
        self.assertEqual(ordinary_failures, [])
        self.assertEqual({path for path, _ in delivery_failures}, {
            ".task/active.md", "_zil_goal/current.md", "goal.md",
        })


if __name__ == "__main__":
    unittest.main()

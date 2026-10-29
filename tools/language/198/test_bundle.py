#!/usr/bin/env python3
"""Exercise the offline tool against actual, isolated Git repositories."""
import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest


TOOL = pathlib.Path(__file__).with_name("bundle.py").resolve()
EVENTS = []
TEMP_ROOT = None
ENV = {**os.environ, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
       "GIT_TERMINAL_PROMPT": "0"}
for variable in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"):
    ENV.pop(variable, None)


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="case198-", dir=TEMP_ROOT)
        self.root = pathlib.Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.run_git(self.source, "init", "-b", "main")
        self.configure(self.source)
        self.write(self.source / "shared.txt", "base\n")
        self.write(self.source / "language.txt", "v0\n")
        self.run_git(self.source, "add", ".")
        self.run_git(self.source, "commit", "-m", "base")
        self.base = self.run_git(self.source, "rev-parse", "HEAD").stdout.strip()
        self.write(self.source / "language.txt", "v1\n")
        self.write(self.source / "shared.txt", "language\n")
        self.run_git(self.source, "commit", "-am", "language")
        self.head = self.run_git(self.source, "rev-parse", "HEAD").stdout.strip()
        self.bundle = self.root / "language.bundle"
        self.manifest = self.root / "language.bundle.json"
        self.destination = "refs/remotes/zil-language/" + self.head

    def tearDown(self):
        self.temp.cleanup()

    def command(self, command, cwd, code=0):
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=ENV, capture_output=True,
                                text=True, encoding="utf-8", errors="replace")
        EVENTS.append({"test": self.id(), "cwd": str(cwd), "command": list(map(str, command)),
                       "expected_exit": code, "exit": result.returncode,
                       "stdout": result.stdout, "stderr": result.stderr})
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        return result

    def run_git(self, repo, *args, code=0):
        return self.command(["git", *args], repo, code=code)

    def tool(self, repo, *args, code=0):
        return self.command([sys.executable, TOOL, "--repo", repo, *args], self.root, code=code)

    def write(self, path, content):
        path.write_text(content, encoding="utf-8")
        EVENTS.append({"test": self.id(), "write": str(path), "content": content})

    def configure(self, repo):
        self.run_git(repo, "config", "user.name", "198 isolated fixture")
        self.run_git(repo, "config", "user.email", "fixture@example.invalid")
        self.run_git(repo, "config", "commit.gpgsign", "false")

    def export(self, base=None, **kwargs):
        args = ["export", "--output", self.bundle, "--expect-commit", self.head]
        if base:
            args += ["--base", base]
        return self.tool(self.source, *args, **kwargs)

    def receiver(self, base=True):
        destination = self.root / "receiver"
        if base:
            self.run_git(self.root, "clone", "--no-local", self.source, destination)
            self.run_git(destination, "switch", "-c", "compile", self.base)
        else:
            destination.mkdir()
            self.run_git(destination, "init", "-b", "compile")
        self.configure(destination)
        return destination

    def import_into(self, repo, **kwargs):
        return self.tool(repo, "import", "--bundle", self.bundle, **kwargs)

    def test_full_bundle_import_and_repeat(self):
        self.export()
        manifest = json.loads(self.manifest.read_text())
        self.assertEqual(manifest["prerequisites"], [])
        repo = self.receiver(base=False)
        self.assertEqual(json.loads(self.import_into(repo).stdout)["status"], "Imported")
        self.assertEqual(self.run_git(repo, "rev-parse", self.destination).stdout.strip(), self.head)
        refs = self.run_git(repo, "for-each-ref", "--format=%(refname)").stdout
        self.assertEqual(refs, self.destination + "\n")
        self.assertEqual(json.loads(self.import_into(repo).stdout)["status"], "AlreadyImported")
        self.assertEqual(self.run_git(repo, "symbolic-ref", "HEAD").stdout.strip(), "refs/heads/compile")

    def test_dirty_worktree_index_fetch_head_and_remotes_unchanged(self):
        self.export()
        repo = self.receiver()
        self.write(repo / "language.txt", "staged local work\n")
        self.run_git(repo, "add", "language.txt")
        self.write(repo / "language.txt", "unstaged local work\n")
        self.write(repo / "untracked.txt", "keep me\n")
        fetch_head = repo / ".git/FETCH_HEAD"
        self.write(fetch_head, "fixture existing FETCH_HEAD\n")
        before = {str(p.relative_to(repo)): p.read_bytes() for p in
                  (repo / "language.txt", repo / "untracked.txt", repo / ".git/index", repo / ".git/config", fetch_head)}
        status = self.run_git(repo, "status", "--porcelain=v1").stdout
        self.import_into(repo)
        self.assertEqual(self.run_git(repo, "status", "--porcelain=v1").stdout, status)
        for name, content in before.items():
            self.assertEqual((repo / name).read_bytes(), content, name)
        self.assertEqual(self.run_git(repo, "rev-parse", "HEAD").stdout.strip(), self.base)

    def test_incremental_and_explicit_fast_forward(self):
        self.export(self.base)
        manifest = json.loads(self.manifest.read_text())
        self.assertIn(self.base, manifest["prerequisites"])
        repo = self.receiver()
        self.import_into(repo)
        self.assertEqual(self.run_git(repo, "rev-parse", "HEAD").stdout.strip(), self.base)
        self.run_git(repo, "merge", "--ff-only", self.destination)
        self.assertEqual(self.run_git(repo, "rev-parse", "HEAD").stdout.strip(), self.head)

    def test_missing_incremental_prerequisite_rejected(self):
        self.export(self.base)
        repo = self.receiver(base=False)
        failed = self.import_into(repo, code=1)
        self.assertIn("prerequisite", failed.stdout.lower())
        self.assertEqual(self.run_git(repo, "for-each-ref", "--format=%(refname)").stdout, "")

    def test_divergence_merges_on_new_branch(self):
        self.export()
        repo = self.receiver()
        self.write(repo / "compile.txt", "speed work\n")
        self.run_git(repo, "add", "compile.txt")
        self.run_git(repo, "commit", "-m", "compile side")
        compile_head = self.run_git(repo, "rev-parse", "HEAD").stdout.strip()
        self.import_into(repo)
        self.run_git(repo, "switch", "-c", "integration/language")
        self.run_git(repo, "merge", "--no-ff", self.destination, "-m", "integrate language")
        self.assertEqual(self.run_git(repo, "rev-parse", "compile").stdout.strip(), compile_head)
        self.assertEqual((repo / "compile.txt").read_text(), "speed work\n")
        self.assertEqual((repo / "language.txt").read_text(), "v1\n")
        parents = self.run_git(repo, "show", "-s", "--format=%P", "HEAD").stdout.split()
        self.assertEqual(parents, [compile_head, self.head])

    def test_conflict_preserves_compile_branch_and_can_abort(self):
        self.export()
        repo = self.receiver()
        self.write(repo / "shared.txt", "compile\n")
        self.run_git(repo, "commit", "-am", "overlapping compile work")
        compile_head = self.run_git(repo, "rev-parse", "HEAD").stdout.strip()
        self.import_into(repo)
        self.run_git(repo, "switch", "-c", "integration/conflict")
        self.run_git(repo, "merge", "--no-ff", self.destination, "-m", "conflict", code=1)
        self.assertTrue(self.run_git(repo, "ls-files", "-u").stdout)
        self.assertEqual(self.run_git(repo, "rev-parse", "compile").stdout.strip(), compile_head)
        self.run_git(repo, "merge", "--abort")
        self.assertEqual(self.run_git(repo, "rev-parse", "HEAD").stdout.strip(), compile_head)
        self.assertEqual((repo / "shared.txt").read_text(), "compile\n")
        self.assertEqual(self.run_git(repo, "status", "--porcelain=v1").stdout, "")

    def test_unrelated_history_is_imported_but_not_merged(self):
        self.export()
        repo = self.receiver(base=False)
        self.write(repo / "unrelated.txt", "independent repository\n")
        self.run_git(repo, "add", ".")
        self.run_git(repo, "commit", "-m", "unrelated root")
        original = self.run_git(repo, "rev-parse", "HEAD").stdout.strip()
        self.import_into(repo)
        self.run_git(repo, "merge-base", "HEAD", self.destination, code=1)
        self.run_git(repo, "merge", "--no-ff", self.destination, "-m", "must reject", code=128)
        self.assertEqual(self.run_git(repo, "rev-parse", "HEAD").stdout.strip(), original)
        self.assertEqual(self.run_git(repo, "status", "--porcelain=v1").stdout, "")

    def test_corrupt_bundle_and_header_mismatch_rejected(self):
        self.export()
        repo = self.receiver(base=False)
        original = self.bundle.read_bytes()
        self.bundle.write_bytes(original[:-1] + bytes([original[-1] ^ 1]))
        self.assertIn("SHA256", self.import_into(repo, code=1).stdout)
        self.bundle.write_bytes(original)
        manifest = json.loads(self.manifest.read_text())
        manifest["source_commit"] = self.base
        self.manifest.write_text(json.dumps(manifest))
        self.assertIn("header", self.import_into(repo, code=1).stdout)
        self.assertEqual(self.run_git(repo, "for-each-ref", "--format=%(refname)").stdout, "")

    def test_existing_destination_is_not_overwritten(self):
        self.export()
        repo = self.receiver()
        self.run_git(repo, "update-ref", self.destination, self.base)
        self.assertIn("already names", self.import_into(repo, code=1).stdout)
        self.assertEqual(self.run_git(repo, "rev-parse", self.destination).stdout.strip(), self.base)

    def test_export_refuses_collision_dirty_wrong_head_empty_and_oversize(self):
        self.write(self.bundle, "do not replace")
        self.export(code=1)
        self.assertEqual(self.bundle.read_text(), "do not replace")
        self.bundle.unlink()
        self.write(self.source / "local.txt", "uncommitted")
        self.assertIn("clean", self.export(code=1).stdout)
        (self.source / "local.txt").unlink()
        self.assertIn("expect-commit", self.tool(self.source, "export", "--output", self.bundle,
                                                "--expect-commit", self.base, code=1).stdout)
        self.assertIn("no new commits", self.export(self.head, code=1).stdout)
        self.assertIn("exceeds", self.tool(self.source, "export", "--output", self.bundle,
                                          "--max-bytes", "1", code=1).stdout)
        self.assertFalse(self.bundle.exists())
        self.assertFalse(self.manifest.exists())


def main():
    global TEMP_ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--temp-root", type=pathlib.Path)
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    if args.temp_root:
        args.temp_root.mkdir(parents=True, exist_ok=True)
        TEMP_ROOT = args.temp_root
    start = time.time()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(BundleTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if args.report:
        source = subprocess.run(["git", "-C", str(TOOL.parent), "rev-parse", "HEAD"],
                                capture_output=True, text=True)
        report = {"status": "Pass" if result.wasSuccessful() else "Fail", "tests": result.testsRun,
                  "failures": len(result.failures), "errors": len(result.errors),
                  "elapsed_seconds": time.time() - start, "source_commit": source.stdout.strip(),
                  "tool_sha256": hashlib.sha256(TOOL.read_bytes()).hexdigest(),
                  "test_sha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                  "failures_detail": [[case.id(), text] for case, text in result.failures],
                  "errors_detail": [[case.id(), text] for case, text in result.errors],
                  "platform": sys.platform, "python": sys.version, "events": EVENTS,
                  "scope": "Actual isolated local Git operations; no compiler build, language acceptance or native Windows claim."}
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())

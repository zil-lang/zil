"""Configuration isolation and evidence guards; not speed evidence."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("extension", Path(__file__).with_name("compile7_source_cache.py"))
extension = importlib.util.module_from_spec(spec)
spec.loader.exec_module(extension)


class SourceCacheControls(unittest.TestCase):
    def plan(self, engine, modes):
        return {
            "schema": "test" + extension.SCHEMA_SUFFIX,
            "binaries": {x: {"path": x} for x in ("A", "B", "clang")},
            "profile": "O0", "units": 1, "jobs": 1, "object_cache": True,
            "query_cache": True, "cache_mode": "incremental", "target": "x86_64-unknown-linux-gnu",
            "rust_profile": "native", "rust_lto": "native", "rust_linker": None,
            "closed_executable": False,
            "compile7_source_cache": {"engine": engine, "modes": modes, "extension_sha256": extension.digest(extension.HERE)},
        }

    def test_configuration_difference_is_only_source_candidate_pair(self):
        modes = {"A": True, "B": False}
        runner = extension.configure("zil", modes)
        plan = self.plan("zil", modes)
        plan["binaries"]["B"]["path"] = "A"
        work = Path("/tmp/not-executed")
        a = runner.command(plan, {"arm": "A", "phase": "build"}, work)
        b = runner.command(plan, {"arm": "B", "phase": "build"}, work)
        index = a.index("--cache-dir")
        self.assertEqual(a[:index] + a[index + 2:], b)
        self.assertIn("--object-cache", b)
        self.assertIn("--query-cache", b)

    def test_rust_arm_is_identical_and_zil_cache_is_independent(self):
        work = Path("/tmp/not-executed")
        on = {"A": True, "B": True}
        off = {"A": False, "B": False}
        a = extension.configure("rust", on)
        b = extension.configure("rust", off)
        self.assertEqual(a.command(self.plan("rust", on), "A", work), b.command(self.plan("rust", off), "A", work))
        argv = b.command(self.plan("rust", off), "B", work)
        self.assertNotIn("--cache-dir", argv)
        self.assertIn("--object-cache", argv)
        self.assertIn("--query-cache", argv)

    def test_changed_extension_or_mode_is_rejected(self):
        modes = {"A": True, "B": False}
        plan = self.plan("zil", modes)
        extension.check_extension(plan, "zil", modes)
        with self.assertRaises(SystemExit):
            extension.check_extension(plan, "zil", {"A": True, "B": True})
        plan["compile7_source_cache"]["extension_sha256"] = "0" * 64
        with self.assertRaises(SystemExit):
            extension.check_extension(plan, "zil", modes)

    def test_ordinary_runner_cannot_silently_accept_extension_plan(self):
        self.assertTrue(extension.configure("zil", {"A": True, "B": True}).SCHEMA.endswith(extension.SCHEMA_SUFFIX))
        with self.assertRaises(ValueError):
            extension.configure("zil", {"A": 1, "B": True})


if __name__ == "__main__":
    unittest.main()

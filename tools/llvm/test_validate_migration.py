#!/usr/bin/env python3
"""Synthetic refusal/logging controls for the migration runner; no ZIL execution."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("migration", Path(__file__).with_name("validate_migration.py"))
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


def report(warm=False):
    item = {"backend": "llvm-api", "llvm_version": "23.1.4", "triple": "x86_64-unknown-linux-gnu",
            "parse_ns": 0, "input_kind": "object-cache" if warm else "builder",
            "cache": {"state": "hit" if warm else "miss"}, "verify_before_after": True,
            "pipeline": "default<O0>", "llvm_verification_this_request": False,
            "producer_verified_before_after": True}
    return {"status": "Success", "verification_runs": 1, "commands": [["clang", "object.o"]],
            "native_codegen": {"backend": "llvm-api", "fallback": None, "actual_units": 2,
                               "object_reports": [item, copy.deepcopy(item)],
                               "object_cache": {"requested": True, "enabled": True, "hits": 2 if warm else 0,
                                                "misses": 0 if warm else 2, "stored": 0 if warm else 2,
                                                "corrupt": 0, "write_failures": 0}}}


class Controls(unittest.TestCase):
    def check(self, value, warm=False):
        return migration.verify_build(value, "23.1.4", "O0", 4, warm)

    def test_synthetic_cold_and_warm_control_shapes(self):
        self.assertEqual(self.check(report()), 2)
        self.assertEqual(self.check(report(True), True), 2)

    def test_current_verify_cannot_be_replaced_by_a_hit(self):
        value = report(True)
        value["verification_runs"] = 0
        with self.assertRaisesRegex(ValueError, "current Verify"):
            self.check(value, True)

    def test_old_version_receipt_is_rejected(self):
        value = report()
        value["native_codegen"]["object_reports"][0]["llvm_version"] = "17.0.6"
        with self.assertRaisesRegex(ValueError, "wrong LLVM"):
            self.check(value)

    def test_cold_candidate_cannot_hit_baseline_objects(self):
        value = report(True)
        with self.assertRaisesRegex(ValueError, "cache counts"):
            self.check(value)

    def test_missing_unit_receipts_and_zero_work_are_rejected(self):
        for count in (0, 1):
            value = report()
            value["native_codegen"]["object_reports"] = value["native_codegen"]["object_reports"][:count]
            with self.assertRaisesRegex(ValueError, "missing object"):
                self.check(value)
        value = report()
        value["native_codegen"]["actual_units"] = 0
        with self.assertRaisesRegex(ValueError, "unit count"):
            self.check(value)

    def test_cached_objects_cannot_claim_current_llvm_verification(self):
        value = report(True)
        value["native_codegen"]["object_reports"][0]["llvm_verification_this_request"] = True
        with self.assertRaisesRegex(ValueError, "misleading cache"):
            self.check(value, True)

    def test_changed_pipeline_and_text_parser_are_rejected(self):
        for key, value in (("pipeline", "default<O0>,function(mem2reg)"), ("parse_ns", 1)):
            candidate = report()
            candidate["native_codegen"]["object_reports"][0][key] = value
            with self.assertRaises(ValueError):
                self.check(candidate)

    def test_nonzero_exit_preserves_raw_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            commands = migration.Commands(Path(directory) / "logs", 5)
            with self.assertRaisesRegex(ValueError, "unexpected exit"):
                commands.run("failure", [sys.executable, "-c", "import sys;print('kept');sys.exit(7)"], os.environ.copy())
            record = json.loads(next((Path(directory) / "logs").glob("*.json")).read_text())
            self.assertEqual(record["status"], "Fail")
            self.assertEqual(record["exit_code"], 7)
            self.assertEqual(Path(record["stdout"]["path"]).read_text(), "kept\n")

    def test_timeout_is_not_a_valid_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            commands = migration.Commands(Path(directory) / "logs", 0.05)
            with self.assertRaisesRegex(ValueError, "timed out"):
                commands.run("timeout", [sys.executable, "-c", "import time;time.sleep(30)"], os.environ.copy(), expected=-9)
            self.assertTrue(commands.records[0]["timed_out"])
            self.assertEqual(commands.records[0]["status"], "Fail")


if __name__ == "__main__":
    unittest.main()

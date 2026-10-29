"""Runner arithmetic/failure controls; synthetic records, not compiler evidence."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("cperf_bench", Path(__file__).with_name("bench.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class RunnerControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zil-cperf-runner-")
        self.root = Path(self.temp.name)
        self.plan_path = self.root / "plan.json"
        self.plan_path.write_text("{}\n")
        for snapshot in ("base", "private", "public"):
            source = self.root / "corpus/x" / snapshot / "zil"
            source.mkdir(parents=True)
            (source / "main.zil").write_text("fn main()->i32=>0\n")
        self.case = {"id": "x", "root": "corpus/x", "weight": 1.0, "inputs": []}
        self.plan = {"schema": bench.SCHEMA, "source_identity": {}, "selected_cases": ["x"],
                     "selected_original_weight": 1, "cases": [self.case], "phases": ["check"],
                     "request_weights": bench.REQUESTS, "profile": "O0", "units": 1, "jobs": 1,
                     "object_cache": False, "cpus": [0], "timeout_seconds": 1,
                     "binaries": {k: {"path": "never-executed"} for k in ("A", "B", "clang")},
                     "scope": "synthetic test", "bootstrap_rule": "synthetic test", "seed": 3, "blocks": 3}
        self.plan["order"] = [{"block": block, "phase": "check", "case": "x", "position": pos, "arm": arm}
                              for block in range(3) for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
        self.seq = self.plan["order"][0]

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def timing(argv, stdout, stderr, cpus, timeout):
        stdout.write_text(json.dumps({"status": "Success", "verification_runs": 1}))
        stderr.write_text("")
        return {"exit": 0, "timeout": False, "wall_seconds": 1.0, "cpu_user": .2, "cpu_system": .1}

    def one(self, timing):
        work, logs = self.root / "work", self.root / "logs"
        logs.mkdir()
        with patch.object(bench.legacy, "timed", side_effect=timing):
            return bench.one(self.plan, self.root, self.case, self.seq, "cold_process", work, logs, "0")

    def test_check_requires_current_verifier(self):
        def skipped(*args):
            result = self.timing(*args)
            args[1].write_text(json.dumps({"status": "Success", "verification_runs": 0}))
            return result
        self.assertFalse(self.one(skipped)["pass"])

    def test_timeout_and_raw_output_are_retained(self):
        def timeout(*args):
            return {**self.timing(*args), "timeout": True, "exit": -9}
        record = self.one(timeout)
        self.assertFalse(record["pass"])
        self.assertTrue(record["timeout"])
        self.assertTrue((self.root / "logs/0.stdout.json").exists())

    def test_source_change_during_request_is_not_scored(self):
        def changed(*args):
            result = self.timing(*args)
            (self.root / "work/src/main.zil").write_text("fn main()->i32=>1\n")
            return result
        record = self.one(changed)
        self.assertFalse(record["pass"])
        self.assertNotEqual(record["source_before"], record["source_after"])

    def test_exception_is_a_failure_row_and_disposable_work_is_removed(self):
        out = self.root / "run"
        out.mkdir()
        with patch.object(bench.legacy, "timed", side_effect=OSError("injected spawn failure")), contextlib.redirect_stdout(io.StringIO()):
            result = bench.execute(self.plan, self.root, out, [self.seq])
        rows = [json.loads(line) for line in (out / "records.jsonl").read_text().splitlines()]
        self.assertEqual(len(rows), 4)
        self.assertEqual(result["failures"], [0, 1, 2, 3])
        self.assertTrue(all("injected spawn failure" in row["error"] for row in rows))
        self.assertFalse(any(out.glob("s*")))
        self.assertTrue((self.root / "corpus/x/base/zil/main.zil").exists())

    def test_weighted_absolute_costs_precede_ratio(self):
        plan = {"cases": [{"id": "small", "weight": .5}, {"id": "big", "weight": .5}], "request_weights": {"cold": 1}}
        rows = [{"block": 0, "case": case, "request": "cold", "arm": arm, "wall_seconds": value}
                for case, values in (("small", (1, 2)), ("big", (100, 50))) for arm, value in zip("AB", values) for _ in range(2)]
        result = bench.legacy.aggregate(plan, rows, [0], "wall_seconds")
        self.assertAlmostEqual(result["B_over_A"], 52 / 101)
        self.assertNotAlmostEqual(result["B_over_A"], (2 + .5) / 2)

    def rows(self):
        return [{**seq, "request": request, "snapshot": bench.SNAPSHOTS[request], "pass": True,
                 "plan_sha256": bench.sha(self.plan_path), "wall_seconds": 10 if seq["arm"] == "A" else 5,
                 "cpu_user": 4 if seq["arm"] == "A" else 2, "cpu_system": 0, "checks": []}
                for seq in self.plan["order"] for request in bench.REQUESTS]

    def summary(self, rows, name="summary.json"):
        records, out = self.root / "rows.jsonl", self.root / name
        records.write_text("".join(json.dumps(row) + "\n" for row in rows))
        args = argparse.Namespace(plan=self.plan_path, records=records, out=out)
        with patch.object(bench, "load", return_value=(self.plan, self.root)), contextlib.redirect_stdout(io.StringIO()):
            bench.summarize(args)
        return json.loads(out.read_text())

    def test_complete_balanced_sample_has_expected_ratio(self):
        result = self.summary(self.rows())
        self.assertTrue(result["coverage_complete"])
        self.assertEqual(result["by_phase"]["check"]["wall"]["B_over_A"], .5)
        self.assertEqual(result["by_phase"]["check"]["exploratory_paired_block_bootstrap95"], [.5, .5])
        self.assertEqual(result["product_gates"], "NotRun")

    def test_missing_row_preserves_failure_summary_without_score(self):
        with self.assertRaises(SystemExit):
            self.summary(self.rows()[:-1])
        result = json.loads((self.root / "summary.json").read_text())
        self.assertEqual(result["verdict"], "IncompleteOrFailed_NoScore")
        self.assertEqual(result["by_phase"], {})

    def test_nonfinite_time_preserves_failure_summary_without_score(self):
        rows = self.rows()
        rows[0]["wall_seconds"] = float("nan")
        with self.assertRaises(SystemExit):
            self.summary(rows)
        result = json.loads((self.root / "summary.json").read_text())
        self.assertEqual(result["failures"], [0])
        self.assertEqual(result["by_phase"], {})


if __name__ == "__main__":
    unittest.main()

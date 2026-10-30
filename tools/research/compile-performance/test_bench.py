"""Runner arithmetic/failure controls; synthetic records, not compiler evidence."""
import argparse
import contextlib
import importlib.util
import io
import json
import subprocess
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
                     "object_cache": False, "cpus": [0], "timeout_seconds": 1, "native_timeout_seconds": 1,
                     "binaries": {k: {"path": "never-executed"} for k in ("A", "B", "clang")},
                     "scope": "synthetic test", "bootstrap_rule": "synthetic test", "seed": 3, "blocks": 3}
        self.plan["order"] = [{"block": block, "phase": "check", "case": "x", "position": pos, "arm": arm}
                              for block in range(3) for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
        self.plan["fixtures_sha256"] = bench.file_hashes(self.root / "corpus")
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

    def rows(self, order=None, directory=None):
        directory = directory or self.root
        logs = directory / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        result = []
        for seq in self.plan["order"] if order is None else order:
            for request in bench.REQUESTS:
                index = len(result)
                snapshot = bench.SNAPSHOTS[request]
                work = directory / ("s" + str(index // 4))
                report = {"status": "Success", "verification_runs": 1, "phase": "Verify",
                          "native_codegen": None, "commands": [], "disk_cache": None, "diagnostics": []}
                stdout, stderr = logs / f"{index}.json", logs / f"{index}.err"
                stdout.write_text(json.dumps(report)); stderr.write_text("")
                source = bench.file_hashes(self.root / self.case["root"] / snapshot / "zil")
                row = {**seq, "request": request, "snapshot": snapshot, "pass": True,
                       "index": index, "exit": 0, "timeout": False, "work": str(work),
                       "argv": bench.command(self.plan, seq, work), "source_before": source, "source_after": source,
                       "stdout": stdout.name, "stderr": stderr.name,
                       "stdout_sha256": bench.sha(stdout), "stderr_sha256": bench.sha(stderr),
                       "plan_sha256": bench.sha(self.plan_path), "wall_seconds": 10 if seq["arm"] == "A" else 5,
                       "cpu_user": 4 if seq["arm"] == "A" else 2, "cpu_system": 0, "checks": []}
                row.update({"reported_" + k: v for k, v in report.items()})
                result.append(row)
        return result

    def preflight(self):
        directory = self.root / "preflight"
        rows = self.rows(bench.preflight_order(self.plan), directory)
        records = directory / "records.jsonl"
        records.write_text("".join(json.dumps(row) + "\n" for row in rows))
        receipt = directory / "result.json"
        receipt.write_text(json.dumps({"pass": True, "failures": [], "rows": len(rows),
                                      "plan_sha256": bench.sha(self.plan_path), "records_sha256": bench.sha(records)}))
        return receipt

    def summary(self, rows, name="summary.json", mutate_receipt=None, mutate_preflight=None, completion=True):
        records, out = self.root / "rows.jsonl", self.root / name
        records.write_text("".join(json.dumps(row) + "\n" for row in rows))
        preflight = self.preflight()
        receipt = {"pass": True, "failures": [], "rows": len(rows), "plan_sha256": bench.sha(self.plan_path),
                   "records_sha256": bench.sha(records), "preflight_sha256": bench.sha(preflight)}
        if mutate_receipt:
            mutate_receipt(receipt)
        if mutate_preflight:
            mutate_preflight(preflight)
        if completion:
            (self.root / "complete.json").write_text(json.dumps(receipt))
        args = argparse.Namespace(plan=self.plan_path, records=records, preflight=preflight, out=out)
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

    def assert_no_score(self, rows, **kwargs):
        with self.assertRaises(SystemExit):
            self.summary(rows, **kwargs)
        result = json.loads((self.root / kwargs.get("name", "summary.json")).read_text())
        self.assertEqual(result["verdict"], "IncompleteOrFailed_NoScore")
        self.assertEqual(result["by_phase"], {})
        return result

    def test_missing_completion_receipt_refuses_score(self):
        result = self.assert_no_score(self.rows(), completion=False)
        self.assertFalse(result["completion_receipt_matches"])

    def test_completion_receipt_must_bind_rows_plan_and_preflight(self):
        for field, value in (("records_sha256", "wrong"), ("plan_sha256", "wrong"),
                             ("preflight_sha256", "wrong"), ("rows", 0), ("pass", False), ("failures", [0])):
            with self.subTest(field=field):
                self.assert_no_score(self.rows(), name=field + ".json", mutate_receipt=lambda r: r.update({field: value}))

    def test_current_verifier_is_checked_again_even_with_resealed_log(self):
        rows = self.rows()
        path = self.root / "logs" / rows[0]["stdout"]
        report = json.loads(path.read_text()); report["verification_runs"] = 0
        path.write_text(json.dumps(report))
        rows[0]["stdout_sha256"] = bench.sha(path)
        rows[0]["reported_verification_runs"] = 0
        self.assertEqual(self.assert_no_score(rows)["failures"], [0])

    def test_cli_log_or_report_excerpt_drift_refuses_score(self):
        rows = self.rows()
        (self.root / "logs" / rows[0]["stderr"]).write_text("changed after execution")
        self.assert_no_score(rows, name="stderr.json")
        rows = self.rows()
        rows[0]["reported_diagnostics"] = ["invented"]
        self.assert_no_score(rows, name="excerpt.json")

    def test_equal_before_after_source_must_match_frozen_snapshot(self):
        rows = self.rows()
        rows[0]["source_before"] = rows[0]["source_after"] = {"main.zil": "wrong"}
        self.assert_no_score(rows)

    def test_command_snapshot_and_index_are_bound(self):
        for field, value in (("argv", ["other-compiler"]), ("snapshot", "private"), ("index", 19)):
            with self.subTest(field=field):
                rows = self.rows(); rows[0][field] = value
                self.assert_no_score(rows, name=field + ".json")

    def test_preflight_raw_log_is_rechecked(self):
        def corrupt(path):
            (path.parent / "logs/0.err").write_text("changed")
        result = self.assert_no_score(self.rows(), mutate_preflight=corrupt)
        self.assertFalse(result["preflight_matches"])

    def test_failed_preflight_blocks_measure_before_creating_output(self):
        preflight = self.preflight()
        (preflight.parent / "logs/0.err").unlink()
        args = argparse.Namespace(plan=self.plan_path, preflight=preflight, quiet_window=True, out=self.root / "unstarted")
        with patch.object(bench, "load", return_value=(self.plan, self.root)), self.assertRaises(SystemExit):
            bench.measure(args)
        self.assertFalse(args.out.exists())

    def native_row(self):
        self.plan["object_cache"] = True
        self.seq = {**self.seq, "phase": "build"}
        io_dir = self.root / "corpus/x/io"
        io_dir.mkdir()
        (io_dir / "input.bin").write_bytes(b"in")
        (io_dir / "expected.bin").write_bytes(b"out")
        self.case["inputs"] = [{"path": "corpus/x/io/input.bin",
                                "expected": {s: "corpus/x/io/expected.bin" for s in ("base", "private", "public")}}]
        self.plan["fixtures_sha256"] = bench.file_hashes(self.root / "corpus")
        def compiler(argv, stdout, stderr, cpus, timeout):
            timed = self.timing(argv, stdout, stderr, cpus, timeout)
            report = {"status": "Success", "verification_runs": 1,
                      "native_codegen": {"actual_units": 1, "object_cache": {"enabled": True, "hits": 0}}}
            stdout.write_text(json.dumps(report))
            executable = Path(argv[argv.index("-o") + 1]); executable.write_bytes(b"synthetic-ELF")
            executable.with_suffix(".ll").write_text("synthetic-IR")
            return timed
        native = subprocess.CompletedProcess(["synthetic"], 0, b"out", b"")
        with patch.object(bench.subprocess, "run", return_value=native):
            row = self.one(compiler)
        row.update(index=0, plan_sha256=bench.sha(self.plan_path))
        return row

    def validate_native(self, row):
        plan = {**self.plan, "order": [self.seq]}
        return bench.validate_rows(plan, self.root, self.root / "records.jsonl", [row], [self.seq])[1]

    def test_native_output_checks_and_frozen_oracle_are_rechecked(self):
        row = self.native_row()
        self.assertEqual(self.validate_native(row), [])
        output = self.root / "logs" / row["checks"][0]["stdout"]
        output.write_bytes(b"bad")
        # Even a resealed check cannot change the independently frozen oracle.
        row["checks"][0]["stdout_sha256"] = bench.sha(output)
        self.assertEqual(self.validate_native(row), [0])

    def test_missing_native_check_or_changed_input_refuses_score(self):
        row = self.native_row()
        checks = row["checks"]
        row["checks"] = []
        self.assertEqual(self.validate_native(row), [0])
        row["checks"] = checks
        checks[0]["input_after_sha256"] = "changed"
        self.assertEqual(self.validate_native(row), [0])

    def test_changed_elf_and_false_cache_contract_are_rejected(self):
        row = self.native_row()
        row["executable_after_sha256"] = "changed"
        self.assertEqual(self.validate_native(row), [0])
        row["executable_after_sha256"] = row["executable_sha256"]
        path = self.root / "logs" / row["stdout"]
        report = json.loads(path.read_text()); report["native_codegen"]["object_cache"]["hits"] = 1
        path.write_text(json.dumps(report))
        row["stdout_sha256"] = bench.sha(path)
        row["reported_native_codegen"] = report["native_codegen"]
        self.assertEqual(self.validate_native(row), [0])


if __name__ == "__main__":
    unittest.main()

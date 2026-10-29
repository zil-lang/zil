"""Synthetic runner controls and tiny process smoke; no compiler/performance experiment."""
import argparse
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("runtime_bench", Path(__file__).with_name("runtime_bench.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class RuntimeControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zil-runtime-controls-")
        self.root = Path(self.temp.name)
        for name in ("inputs", "expected", "sources/x", "logs"):
            (self.root / name).mkdir(parents=True)
        for name in ("inputs/empty.bin", "expected/empty.bin"):
            (self.root / name).write_bytes(b"")
        (self.root / "inputs/x.bin").write_bytes(b"sample")
        (self.root / "expected/x.bin").write_bytes(b"answer")
        (self.root / "sources/x/main.zil").write_text("fn main()->i32=>0\n")
        self.executable = self.root / "never-executed"
        self.executable.write_bytes(b"synthetic executable identity")
        self.identity = bench.common.inventory(self.executable)
        self.case = {"id": "x", "weight": 1, "source": "sources/x", "input": "inputs/x.bin",
                     "expected": "expected/x.bin", "source_sha256": bench.common.file_hashes(self.root / "sources/x")}
        self.plan = {"cases": [self.case], "source_identity": {"synthetic": True}, "cpu": 0,
                     "runtime_timeout_seconds": 1, "build_timeout_seconds": 1, "blocks": 3, "seed": 1,
                     "scope": "Invented control values, not native performance evidence",
                     "binaries": {arm: self.identity for arm in ("A", "B", "clang")},
                     "fixtures_sha256": bench.fixture_hashes(self.root)}
        self.plan["order"] = [{"block": block, "case": "x", "position": pos, "arm": arm}
                              for block in range(3) for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
        self.plan_path = self.root / "plan.json"
        self.plan_path.write_text(json.dumps(self.plan))

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def native(executable, source, stdout, stderr, cpu, timeout):
        empty = source.name == "empty.bin"
        stdout.write_bytes(b"" if empty else b"answer")
        stderr.write_bytes(b"")
        candidate = executable.parent.name.endswith("-B")
        return {"wall_seconds": .01 if empty else (5 if candidate else 10),
                "cpu_user": 2 if candidate else 4, "cpu_system": 0, "exit": 0,
                "timeout": False, "max_process_rss_kib": 1}

    @staticmethod
    def compiler(argv, stdout, stderr, cpus, timeout):
        executable = Path(argv[argv.index("-o") + 1])
        executable.write_bytes(b"synthetic " + executable.parent.name.encode())
        executable.with_suffix(".ll").write_text("; synthetic, never executed\n")
        stdout.write_text(json.dumps({"status": "Success", "verification_runs": 1, "timings": None}))
        stderr.write_bytes(b"")
        return {"exit": 0, "timeout": False, "wall_seconds": 12345, "cpu_user": 123, "cpu_system": 0}

    def one(self, timing, tag="test"):
        with patch.object(bench, "timed_native", side_effect=timing):
            return bench.run_native(self.plan, self.root, self.identity, "inputs/x.bin", "expected/x.bin", self.root / "logs", tag)

    def build_control(self, compiler=None):
        args = argparse.Namespace(plan=self.plan_path, out=self.root / "build")
        with patch.object(bench, "load", return_value=(self.plan, self.root)), \
             patch.object(bench.common.legacy, "timed", side_effect=compiler or self.compiler), \
             patch.object(bench, "timed_native", side_effect=self.native), contextlib.redirect_stdout(io.StringIO()):
            bench.build(args)
        self.receipt = args.out / "result.json"

    def measure_control(self):
        self.build_control()
        args = argparse.Namespace(plan=self.plan_path, build_receipt=self.receipt,
                                  out=self.root / "measure", quiet_window=True)
        with patch.object(bench, "load", return_value=(self.plan, self.root)), \
             patch.object(bench, "timed_native", side_effect=self.native), contextlib.redirect_stdout(io.StringIO()):
            bench.measure(args)
        self.records = args.out / "records.jsonl"

    def summary(self):
        args = argparse.Namespace(plan=self.plan_path, build_receipt=self.receipt,
                                  records=self.records, out=self.root / "summary.json")
        with patch.object(bench, "load", return_value=(self.plan, self.root)), contextlib.redirect_stdout(io.StringIO()):
            bench.summarize(args)
        return json.loads(args.out.read_text())

    def test_oracle_checks_stdout_and_empty_stderr(self):
        self.assertTrue(self.one(self.native, "good")["pass"])
        for label, wrong_stream in (("wrong-output", "stdout"), ("diagnostic", "stderr")):
            def altered(*args):
                result = self.native(*args)
                args[2 if wrong_stream == "stdout" else 3].write_bytes(b"unexpected")
                return result
            with self.subTest(stream=wrong_stream):
                self.assertFalse(self.one(altered, label)["pass"])
                self.assertTrue((self.root / "logs" / (label + ".stdout.bin")).exists())

    def test_timeout_and_spawn_failure_remain_failure_rows(self):
        def timeout(*args):
            return {**self.native(*args), "timeout": True, "exit": -9}
        row = self.one(timeout)
        self.assertFalse(row["pass"])
        self.assertTrue(row["timeout"])
        self.assertTrue((self.root / "logs/test.stdout.bin").exists())
        def error(*args):
            raise OSError("injected spawn error")
        row = self.one(error, "error")
        self.assertFalse(row["pass"])
        self.assertIn("injected spawn error", row["error"])

    def test_changed_input_is_rejected_before_execution(self):
        (self.root / "inputs/x.bin").write_bytes(b"changed")
        with patch.object(bench, "timed_native") as timer:
            row = bench.run_native(self.plan, self.root, self.identity, "inputs/x.bin", "expected/x.bin", self.root / "logs", "drift")
        self.assertFalse(row["pass"])
        self.assertTrue(row["identity_drift"])
        timer.assert_not_called()

    def test_executable_change_during_execution_is_rejected(self):
        def changed(*args):
            result = self.native(*args)
            self.executable.write_bytes(b"changed executable")
            return result
        row = self.one(changed)
        self.assertFalse(row["pass"])
        self.assertNotEqual(row["executable"], row["executable_after"])

    def test_timer_captures_file_io_and_wait4_usage(self):
        stdout, stderr = self.root / "copy.stdout", self.root / "copy.stderr"
        result = bench.timed_native(Path("/bin/cat"), self.root / "inputs/x.bin", stdout, stderr,
                                    min(os.sched_getaffinity(0)), 1)
        self.assertEqual(result["exit"], 0)
        self.assertFalse(result["timeout"])
        self.assertEqual(stdout.read_bytes(), b"sample")
        self.assertEqual(stderr.read_bytes(), b"")
        self.assertGreater(result["wall_seconds"], 0)
        self.assertGreaterEqual(result["cpu_user"] + result["cpu_system"], 0)

    def test_timer_kills_process_group_and_keeps_partial_logs(self):
        script = self.root / "timeout-control"
        script.write_text("#!/bin/sh\nprintf 'started'\nsleep 10\n")
        script.chmod(0o700)
        stdout, stderr = self.root / "timeout.stdout", self.root / "timeout.stderr"
        result = bench.timed_native(script, self.root / "inputs/empty.bin", stdout, stderr,
                                    min(os.sched_getaffinity(0)), .02)
        self.assertTrue(result["timeout"])
        self.assertNotEqual(result["exit"], 0)
        self.assertTrue(stdout.exists())
        self.assertTrue(stderr.exists())

    def test_weighted_absolute_costs_precede_ratio(self):
        plan = {"cases": [{"id": "small", "weight": .5}, {"id": "big", "weight": .5}]}
        rows = [{"block": 0, "case": case, "arm": arm, "wall_seconds": value}
                for case, values in (("small", (1, 2)), ("big", (100, 50)))
                for arm, value in zip("AB", values) for _ in range(2)]
        result = bench.aggregate(plan, rows, [0], "wall_seconds")
        self.assertEqual(result["A"], 50.5)
        self.assertEqual(result["B"], 26)
        self.assertAlmostEqual(result["B_over_A"], 52/101)
        self.assertNotAlmostEqual(result["B_over_A"], 1.25)

    def test_pair_means_precede_median_and_outlier_is_retained(self):
        rows = [{"block": block, "case": "x", "arm": arm, "wall_seconds": value * factor}
                for block, values in enumerate(((1, 19), (7, 9), (100, 100)))
                for arm, factor in (("A", 1), ("B", 2)) for value in values]
        result = bench.aggregate(self.plan, rows, [0, 1, 2], "wall_seconds")
        self.assertEqual(result["A"], 10)
        self.assertEqual(result["B"], 20)
        self.assertEqual(bench.aggregate(self.plan, rows, [2, 2, 2], "wall_seconds")["A"], 100)

    def test_build_and_startup_costs_do_not_enter_runtime_score(self):
        self.measure_control()
        result = self.summary()
        self.assertTrue(result["coverage_complete"])
        self.assertTrue(result["completion_receipt_matches"])
        self.assertEqual(result["rows"], 12)
        self.assertEqual(result["wall"]["A"], 10)
        self.assertEqual(result["wall"]["B"], 5)
        self.assertEqual(result["cpu"]["B_over_A"], .5)
        self.assertEqual(result["exploratory_paired_block_bootstrap95"], [.5, .5])
        self.assertEqual(result["startup_diagnostics"]["x"]["A"]["single_preflight_empty_seconds"], .01)
        self.assertEqual(result["product_gates"], "NotRun")
        build_rows = [json.loads(x) for x in (self.receipt.parent / "builds.jsonl").read_text().splitlines()]
        self.assertEqual(len(build_rows), 2)
        self.assertTrue(all(row["build_process"]["wall_seconds"] == 12345 for row in build_rows))
        self.assertFalse(any(self.receipt.parent.rglob("*.ll")))
        self.assertTrue(all(row["llvm_sha256"] for row in build_rows))

    def test_source_drift_during_build_prevents_native_execution(self):
        def changed(*args):
            result = self.compiler(*args)
            (self.root / "sources/x/main.zil").write_text("changed\n")
            return result
        self.build_control(changed)
        result = json.loads(self.receipt.read_text())
        self.assertFalse(result["pass"])
        with self.assertRaises(SystemExit):
            bench.built(self.plan, self.plan_path, self.receipt)
        self.assertFalse(any((self.receipt.parent / "logs").glob("*.warmup.*")))

    def test_modified_native_preflight_log_is_rejected(self):
        self.build_control()
        (self.receipt.parent / "logs/x-A.warmup.stdout.bin").write_bytes(b"changed")
        with self.assertRaises(SystemExit):
            bench.built(self.plan, self.plan_path, self.receipt)

    def test_missing_row_and_changed_raw_log_suppress_scores(self):
        self.measure_control()
        lines = self.records.read_text().splitlines()
        self.records.write_text("\n".join(lines[:-1]) + "\n")
        with self.assertRaises(SystemExit):
            self.summary()
        result = json.loads((self.root / "summary.json").read_text())
        self.assertFalse(result["coverage_complete"])
        self.assertNotIn("wall", result)
        (self.root / "summary.json").unlink()
        self.records.write_text("\n".join(lines) + "\n")
        (self.records.parent / "logs/0000.stdout.bin").write_bytes(b"corrupted")
        with self.assertRaises(SystemExit):
            self.summary()
        result = json.loads((self.root / "summary.json").read_text())
        self.assertEqual(result["failures"], [0])
        self.assertNotIn("wall", result)

    def test_nonfinite_or_rewritten_time_suppresses_scores(self):
        self.measure_control()
        rows = [json.loads(line) for line in self.records.read_text().splitlines()]
        rows[0]["wall_seconds"] = float("nan")
        self.records.write_text("".join(json.dumps(row) + "\n" for row in rows))
        with self.assertRaises(SystemExit):
            self.summary()
        result = json.loads((self.root / "summary.json").read_text())
        self.assertFalse(result["completion_receipt_matches"])
        self.assertEqual(result["failures"], [0])
        self.assertNotIn("wall", result)

    def test_prepare_freezes_base_sources_and_all_runtime_inputs(self):
        upstream_root = self.root / "upstream"
        upstream = {"schema": "zil-compile-performance/1", "cases": [], "fixtures_sha256": {},
                    "dependencies_sha256": {p: bench.sha(bench.ROOT / p) for p in bench.common.DEPENDENCIES}}
        for name in bench.INPUT_BYTES:
            path = upstream_root / "corpus" / name / "base/zil/main.zil"
            path.parent.mkdir(parents=True)
            path.write_text("fn main()->i32=>0\n")
            upstream["cases"].append({"id": name, "root": "corpus/" + name, "weight": .2,
                                      "kind": "synthetic control", "modules_requested": 4, "width": 2})
            upstream["fixtures_sha256"][name + "/base/zil/main.zil"] = bench.sha(path)
        upstream_path = upstream_root / "plan.json"
        upstream_path.write_text(json.dumps(upstream))
        args = argparse.Namespace(out=self.root / "frozen", corpus_plan=upstream_path,
                                  baseline=self.executable, candidate=self.executable, clang=self.executable,
                                  baseline_source="synthetic A", candidate_source="synthetic B", cpu=None,
                                  blocks=5, seed=1, timeout=1, tool=[])
        with patch.object(bench.common, "git", side_effect=lambda *args: "" if args[0] == "status" else "synthetic"), \
             patch.object(bench.common, "source_identity", side_effect=lambda ref: {"commit": ref}), \
             patch.object(bench.common, "capture", return_value={"synthetic": True}), \
             patch.object(bench, "INPUT_BYTES", {name: 7 for name in bench.INPUT_BYTES}), \
             contextlib.redirect_stdout(io.StringIO()):
            bench.prepare(args)
        plan_path = args.out / "plan.json"
        plan, root = bench.load(plan_path)
        self.assertEqual(plan["expected_rows"], 100)
        self.assertEqual([case["input_bytes"] for case in plan["cases"]], [7]*5)
        self.assertEqual(len(list((root / "sources").rglob("*.zil"))), 5)
        self.assertFalse((root / "corpus").exists())
        (root / "inputs/tree4x2.bin").write_bytes(b"changed")
        with self.assertRaises(SystemExit):
            bench.load(plan_path)


if __name__ == "__main__":
    unittest.main()

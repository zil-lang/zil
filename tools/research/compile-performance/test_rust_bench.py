"""CP08 runner controls using synthetic compiler/native records, not speed evidence."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("cp08", Path(__file__).with_name("rust_bench.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class RustBenchmarkControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zil-cp08-test-")
        self.root = Path(self.temp.name)
        for snapshot in ("base", "private", "public"):
            for lang, ext in (("rust", "rs"), ("zil", "zil")):
                folder = self.root / "corpus/x" / snapshot / lang
                folder.mkdir(parents=True)
                (folder / ("main." + ext)).write_text(snapshot + " " + lang)
        (self.root / "inputs").mkdir(); (self.root / "expected").mkdir()
        for name, data in (("empty", b""), ("x", b"abc")):
            (self.root / f"inputs/{name}.bin").write_bytes(data)
            (self.root / f"expected/{name}.bin").write_bytes(data)
        self.case = {"id": "x", "root": "corpus/x", "weight": 1., "input": "inputs/x.bin", "expected": "expected/x.bin",
                     "inputs": [{"path": "inputs/x.bin", "expected": {s: "expected/x.bin" for s in ("base", "private", "public")}}]}
        self.plan = {"schema": bench.SCHEMA, "source_identity": {}, "versions": {}, "profile": "O0", "cache_mode": "cold",
                     "rust_profile": "native", "rust_linker": None, "units": 1, "jobs": 1, "target": bench.TARGET,
                     "cpus": [0], "cpu": 0, "timeout_seconds": 1, "runtime_timeout_seconds": 1,
                     "binaries": {k: {"path": "synthetic-" + k} for k in ("A", "B", "clang")},
                     "request_weights": {"cold_process": 1.}, "cases": [self.case], "selected_original_weight": 1.,
                     "blocks": 3, "seed": 8, "scope": "synthetic build", "runtime_scope": "synthetic runtime", "comparison_limits": "test",
                     "fixtures_sha256": bench.fixtures(self.root)}
        self.plan["order"] = bench.order_for(self.plan["cases"], 3, 8)
        self.plan_path = self.root / "plan.json"
        bench.save(self.plan_path, self.plan)

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def compiler(argv, stdout, stderr, cpus, timeout):
        output = Path(argv[argv.index("-o") + 1])
        output.write_bytes(b"synthetic executable")
        output.with_suffix(".ll").write_text("synthetic IR")
        incremental = "--object-cache" in argv
        report = {"status": "Success", "verification_runs": 1, "timings": None,
                  "native_codegen": {"backend": "llvm-api", "actual_units": 1,
                                     "object_cache": {"requested": incremental, "enabled": incremental, "hits": 0}}}
        stdout.write_text(json.dumps(report) if "--json" in argv else "")
        stderr.write_bytes(b"")
        return {"exit": 0, "timeout": False, "wall_seconds": 5. if "--json" in argv else 10.,
                "cpu_user": 2. if "--json" in argv else 4., "cpu_system": 0., "max_process_rss_kib": 1}

    @staticmethod
    def native(plan, root, executable, source, expected, logs, tag):
        stdout, stderr = logs / (tag + ".out"), logs / (tag + ".err")
        stdout.write_bytes((root / expected).read_bytes()); stderr.write_bytes(b"")
        return {"pass": True, "exit": 0, "timeout": False, "wall_seconds": 1., "cpu_user": .5, "cpu_system": 0.,
                "executable": executable, "executable_after": executable, "input": source, "expected": expected,
                "input_sha256": bench.sha(root / source), "input_after_sha256": bench.sha(root / source),
                "expected_stdout_sha256": bench.sha(root / expected), "stdout_sha256": bench.sha(stdout), "stderr_sha256": bench.sha(stderr),
                "stdout": stdout.name, "stderr": stderr.name}

    def execute(self, name="preflight", warmup=True, compiler=None):
        out = bench.fresh(self.root / name)
        with patch.object(bench.common.legacy, "timed", side_effect=compiler or self.compiler), \
             patch.object(bench.runtime, "run_native", side_effect=self.native), contextlib.redirect_stdout(io.StringIO()):
            result = bench.execute(self.plan, self.root, out, bench.preflight_order(self.plan) if warmup else self.plan["order"], warmup=warmup)
        bench.save(out / ("result.json" if warmup else "complete.json"), result)
        return out

    def measure(self, kind="build"):
        pre = self.execute()
        out = self.root / kind
        args = argparse.Namespace(action="measure" if kind == "build" else "measure-runtime", plan=self.plan_path,
                                  preflight=pre / "result.json", out=out, quiet_window=True)
        with patch.object(bench, "load", return_value=(self.plan, self.root)), \
             patch.object(bench.common.legacy, "timed", side_effect=self.compiler), \
             patch.object(bench.runtime, "run_native", side_effect=self.native), contextlib.redirect_stdout(io.StringIO()):
            bench.measure(args)
        return pre, out

    def summarize(self, pre, out, kind="build"):
        args = argparse.Namespace(plan=self.plan_path, preflight=pre / "result.json", records=out / "records.jsonl",
                                  out=out / "summary.json", kind=kind)
        with patch.object(bench, "load", return_value=(self.plan, self.root)), contextlib.redirect_stdout(io.StringIO()):
            bench.summarize(args)
        return json.loads(args.out.read_text())

    def freeze_tools(self, explicit=False, later_path=False):
        """Real file inventory/load; only the synthetic rustc probe is mocked."""
        sdk = self.root / "sdk"
        bindir = sdk / "bin"
        bindir.mkdir(parents=True)
        (sdk / "lib").mkdir()
        for name in ("rustc", "zil", "clang-real", "lld-one", "lld-two", "explicit-linker", "cc"):
            path = bindir / name
            path.write_bytes(b"same synthetic tool")
            path.chmod(0o755)
        (bindir / "clang").symlink_to("clang-real")
        (bindir / "ld.lld").symlink_to("lld-one")
        library = sdk / "lib/libLLVM.so"
        library.write_bytes(b"synthetic LLVM")
        earlier = sdk / "earlier"
        earlier.mkdir()
        search_path = str(earlier) + ":" + str(bindir) if later_path else str(bindir)
        binaries = {k: bench.tool_identity(bindir / name) for k, name in (("A", "rustc"), ("B", "zil"), ("clang", "clang"))}
        with patch.dict(bench.os.environ, {"PATH": search_path}):
            bench.os.environ.pop("ZIL_LINUX_LINKER", None)
            if explicit:
                bench.os.environ["ZIL_LINUX_LINKER"] = str(bindir / "explicit-linker")
            with patch.object(bench.common, "capture", return_value={"exit": 0, "stdout": str(sdk), "stderr": ""}):
                extra, linkage = bench.tool_inventory(binaries["A"]["path"], binaries["clang"]["path"], [library], None)
        self.plan.update(binaries=binaries, extra_tools=extra, linkage=linkage,
                         runner_sha256=bench.sha(Path(bench.__file__)), helper_sha256={})
        self.plan_path.write_text(json.dumps(self.plan))
        bench.load(self.plan_path, check_environment=False)
        return sdk

    def test_explicit_zil_linker_content_drift_rejected(self):
        sdk = self.freeze_tools(explicit=True)
        linker = sdk / "bin/explicit-linker"
        self.assertEqual(self.plan["linkage"]["linker_candidates"]["zil_explicit"], str(linker))
        self.assertIn(str(linker), [t["path"] for t in self.plan["extra_tools"]])
        linker.write_bytes(b"changed executable at unchanged override path")
        with self.assertRaisesRegex(SystemExit, "Frozen toolchain changed"):
            bench.load(self.plan_path, check_environment=False)

    def test_linker_symlink_retarget_rejected_with_old_target_unchanged(self):
        sdk = self.freeze_tools()
        alias, old = sdk / "bin/ld.lld", sdk / "bin/lld-one"
        before = bench.tool_identity(old)
        alias.unlink()
        alias.symlink_to("lld-two")  # Both targets have identical bytes.
        self.assertEqual(bench.tool_identity(old), before)
        with self.assertRaisesRegex(SystemExit, "Frozen toolchain changed"):
            bench.load(self.plan_path, check_environment=False)

    def test_invocation_aliases_preserved_and_compiler_retarget_rejected(self):
        sdk = self.freeze_tools()
        clang = self.plan["binaries"]["clang"]
        self.assertEqual(clang["path"], str(sdk / "bin/clang"))
        self.assertEqual(clang["resolved_path"], str(sdk / "bin/clang-real"))
        self.assertEqual(clang["symlink_target"], "clang-real")
        argv = bench.command(self.plan, "B", self.root / "work")
        self.assertEqual(argv[argv.index("--clang") + 1], str(sdk / "bin/clang"))
        lld = next(t for t in self.plan["extra_tools"] if t["path"] == str(sdk / "bin/ld.lld"))
        self.assertEqual(lld["resolved_path"], str(sdk / "bin/lld-one"))
        (sdk / "bin/clang").unlink()
        (sdk / "bin/clang").symlink_to("lld-two")
        with self.assertRaisesRegex(SystemExit, "Frozen toolchain changed"):
            bench.load(self.plan_path, check_environment=False)

    def test_new_higher_priority_path_linker_rejected(self):
        sdk = self.freeze_tools(later_path=True)
        earlier = sdk / "earlier/cc"
        earlier.write_bytes(b"new selected compiler driver")
        earlier.chmod(0o755)
        with self.assertRaisesRegex(SystemExit, "Frozen linker selection changed"):
            bench.load(self.plan_path, check_environment=False)

    def test_native_rust_cgu_and_cold_profile_do_not_add_caches(self):
        work = self.root / "work"
        rust, zil = bench.command(self.plan, "A", work), bench.command(self.plan, "B", work)
        self.assertFalse(any("codegen-units" in x or "incremental=" in x for x in rust))
        self.assertNotIn("--cache-dir", zil); self.assertNotIn("--object-cache", zil)
        self.assertIn("overflow-checks=on", rust); self.assertIn("panic=abort", rust)
        self.plan.update(profile="O2", cache_mode="incremental", rust_profile="matched", units=8, rust_linker="/explicit/clang")
        rust, zil = bench.command(self.plan, "A", work), bench.command(self.plan, "B", work)
        for option in ("opt-level=2", "codegen-units=8", "linker=/explicit/clang", "incremental=" + str(work / "rust-incremental")):
            self.assertIn(option, rust)
        self.assertIn("--cache-dir", zil); self.assertIn("--object-cache", zil)

    def test_abba_baab_order_has_balanced_pairs(self):
        self.assertEqual([r["arm"] for r in self.plan["order"]], list("ABBABAABABBA"))

    def test_missing_verifier_or_enabled_timings_rejected(self):
        for value in (0, 2):
            self.assertFalse(bench.report_ok(self.plan, "cold_process", {"status": "Success", "verification_runs": value}))
        report = {"status": "Success", "verification_runs": 1, "timings": {}, "native_codegen": {"backend": "llvm-api"}}
        self.assertFalse(bench.report_ok(self.plan, "cold_process", report))

    def test_requested_incremental_cache_cannot_silently_fall_back(self):
        self.plan["cache_mode"] = "incremental"
        report = {"status": "Success", "verification_runs": 1, "native_codegen": {"backend": "llvm-api", "actual_units": 8,
                  "object_cache": {"requested": True, "enabled": True, "hits": 0}}}
        self.assertTrue(bench.report_ok(self.plan, "cold_process", report))
        self.assertFalse(bench.report_ok(self.plan, "unchanged", report))
        report["native_codegen"]["object_cache"]["hits"] = 8
        self.assertTrue(bench.report_ok(self.plan, "unchanged", report))
        self.assertFalse(bench.report_ok(self.plan, "cold_process", report))

    def test_compiler_failure_retains_rows_and_logs(self):
        def failing(*args):
            return {**self.compiler(*args), "exit": 1}
        out = self.execute(compiler=failing)
        self.assertEqual([r["pass"] for r in bench.rows(out / "records.jsonl")], [False, False])
        self.assertEqual(len(list((out / "logs").glob("*.build.stderr"))), 2)
        with self.assertRaises(SystemExit):
            bench.checked_preflight(self.plan, self.root, out / "result.json")

    def test_source_mutation_during_build_is_rejected(self):
        def changing(*args):
            result = self.compiler(*args)
            path = next(Path(a) for a in args[0] if a.endswith("main.rs") or a.endswith("main.zil"))
            path.write_text("mutated")
            return result
        out = self.execute(compiler=changing)
        self.assertTrue(all(not r["pass"] for r in bench.rows(out / "records.jsonl")))

    def test_complete_build_uses_only_measured_rows(self):
        pre, out = self.measure()
        result = self.summarize(pre, out)
        self.assertEqual(result["rows"], 12)
        self.assertEqual(result["wall"]["B_over_A"], .5)
        self.assertEqual(result["cpu"]["B_over_A"], .5)
        self.assertEqual(result["exploratory_paired_block_bootstrap95"], [.5, .5])
        self.assertEqual(result["product_gates"], "NotRun")

    def test_runtime_excludes_compilation_and_warmup(self):
        pre, out = self.measure("runtime")
        result = self.summarize(pre, out, "runtime")
        self.assertEqual(result["wall"]["A"], 1.)
        self.assertEqual(result["wall"]["B"], 1.)
        self.assertEqual(result["rows"], 12)
        self.assertTrue(all(r["sample_kind"] == "runtime" for r in bench.rows(out / "records.jsonl")))

    def test_missing_rows_suppress_score(self):
        pre, out = self.measure()
        path = out / "records.jsonl"
        path.write_text("\n".join(path.read_text().splitlines()[:-1]) + "\n")
        with self.assertRaises(SystemExit):
            self.summarize(pre, out)
        result = json.loads((out / "summary.json").read_text())
        self.assertEqual(result["verdict"], "IncompleteOrFailed_NoScore")
        self.assertNotIn("wall", result)

    def test_changed_compiler_log_suppresses_score(self):
        pre, out = self.measure()
        first = bench.rows(out / "records.jsonl")[0]
        (out / "logs" / first["stderr"]).write_text("changed log")
        with self.assertRaises(SystemExit):
            self.summarize(pre, out)
        self.assertIn(0, json.loads((out / "summary.json").read_text())["failures"])

    def test_runtime_oracle_drift_suppresses_score(self):
        pre, out = self.measure("runtime")
        first = bench.rows(out / "records.jsonl")[0]
        (out / "logs" / first["stdout"]).write_bytes(b"wrong")
        with self.assertRaises(SystemExit):
            self.summarize(pre, out, "runtime")

    def test_preflight_artifact_mutation_is_rejected(self):
        out = self.execute()
        artifact = json.loads((out / "artifacts.json").read_text())[0]
        Path(artifact["executable"]["path"]).write_bytes(b"changed")
        with self.assertRaises(SystemExit):
            bench.checked_preflight(self.plan, self.root, out / "result.json")

    def test_nonfinite_times_and_zero_wall_rejected(self):
        self.assertFalse(bench.valid_time({"wall_seconds": float("nan"), "cpu_user": 1, "cpu_system": 0}))
        self.assertFalse(bench.valid_time({"wall_seconds": 0, "cpu_user": 1, "cpu_system": 0}))


if __name__ == "__main__":
    unittest.main()

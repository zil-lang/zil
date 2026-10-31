#!/usr/bin/env python3
"""Non-author CP31 controls. Synthetic records only; no compiler/performance claim."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
ROOT = Path('/workspace/scratch/c3607737969d/zil')
TOOLS = ROOT / 'tools/research/compile-performance'
COMMIT = 'f95297b7a30c70b6e7387373e0978c02ce644e99'


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, TOOLS / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


extension = module('cp31_extension_independent', 'compile7_source_cache.py')
author = module('cp31_author_controls', 'compile7_source_cache_test.py')
zil_tests = module('cp31_zil_inherited', 'test_bench.py')
rust_tests = module('cp31_rust_inherited', 'test_rust_bench.py')
original_zil = zil_tests.bench
original_rust = rust_tests.bench
MODES = {'A': True, 'B': False}
zil_tests.bench = extension.configure('zil', MODES)
rust_tests.bench = extension.configure('rust', MODES)


def entry(engine, modes=MODES):
    return {'engine': engine, 'modes': dict(modes),
            'extension_sha256': extension.digest(extension.HERE)}


class ZilWrappedControls(zil_tests.RunnerControls):
    """Existing safety controls exercised through the wrapped runner globals."""
    def setUp(self):
        super().setUp()
        self.plan['compile7_source_cache'] = entry('zil')
        self.plan_path.write_text(json.dumps(self.plan))

    def test_disabled_arm_command_cannot_be_resealed_as_enabled(self):
        rows = self.rows()
        selected = next(i for i, row in enumerate(rows) if row['arm'] == 'B')
        row = rows[selected]
        self.assertNotIn('--cache-dir', row['argv'])
        row['argv'] += ['--cache-dir', str(Path(row['work']) / 'source-candidates')]
        self.assertIn(selected, self.assert_no_score(rows)['failures'])

    def test_mode_change_after_recording_invalidates_frozen_rows(self):
        rows = self.rows()
        self.plan['compile7_source_cache']['modes']['B'] = True
        self.plan_path.write_text(json.dumps(self.plan))
        result = self.assert_no_score(rows)
        self.assertEqual(result['failures'], list(range(len(rows))))


class RustWrappedControls(rust_tests.RustBenchmarkControls):
    def setUp(self):
        super().setUp()
        self.plan['compile7_source_cache'] = entry('rust')
        self.plan_path.write_text(json.dumps(self.plan))

    def test_native_rust_cgu_and_cold_profile_do_not_add_caches(self):
        work = self.root / 'work'
        rust = rust_tests.bench.command(self.plan, 'A', work)
        zil = rust_tests.bench.command(self.plan, 'B', work)
        self.assertFalse(any('codegen-units' in value or 'incremental=' in value for value in rust))
        self.assertNotIn('--cache-dir', zil)
        self.assertNotIn('--object-cache', zil)
        self.assertIn('overflow-checks=on', rust)
        self.assertIn('panic=abort', rust)
        self.plan.update(profile='O2', cache_mode='incremental', rust_profile='matched',
                         units=8, rust_linker='/explicit/clang')
        rust = rust_tests.bench.command(self.plan, 'A', work)
        zil = rust_tests.bench.command(self.plan, 'B', work)
        for value in ('opt-level=2', 'codegen-units=8', 'linker=/explicit/clang',
                      'incremental=' + str(work / 'rust-incremental')):
            self.assertIn(value, rust)
        # Intentional experiment difference: only this pair is disabled for B.
        self.assertNotIn('--cache-dir', zil)
        self.assertIn('--object-cache', zil)

    def test_v2_plan_requires_its_original_runner_instead_of_assuming_native(self):
        # The wrapper's schema guard now rejects before the historical loader.
        old = {**self.plan, 'schema': 'zil-rust-compile-performance/2'}
        old.pop('rust_lto')
        self.plan_path.write_text(json.dumps(old))
        with self.assertRaisesRegex(SystemExit, 'compile7 source-cache plan'):
            rust_tests.bench.load(self.plan_path, check_environment=False)

    def test_incremental_source_off_preserves_oracle_and_command_checks(self):
        self.plan['cache_mode'] = 'incremental'
        self.plan_path.write_text(json.dumps(self.plan))
        pre, out = self.measure()
        rows = rust_tests.bench.rows(out / 'records.jsonl')
        b = next(row for row in rows if row['arm'] == 'B')
        self.assertNotIn('--cache-dir', b['argv'])
        self.assertIn('--object-cache', b['argv'])
        self.assertTrue(self.summarize(pre, out)['coverage_complete'])
        b['argv'] += ['--cache-dir', str(Path(b['work']) / 'source-candidates')]
        complete, failures = rust_tests.bench.validate_build_rows(
            self.plan, self.root, out / 'records.jsonl', rows, self.plan['order'], 'build')
        self.assertTrue(complete)
        self.assertEqual(failures, [b['index']])
        b['argv'] = rust_tests.bench.command(self.plan, 'B', Path(b['work']))
        check = b['checks'][0]
        output = out / 'logs' / check['stdout']
        output.write_bytes(b'wrong native result')
        check['stdout_sha256'] = rust_tests.bench.sha(output)
        complete, failures = rust_tests.bench.validate_build_rows(
            self.plan, self.root, out / 'records.jsonl', rows, self.plan['order'], 'build')
        self.assertTrue(complete)
        self.assertEqual(failures, [b['index']])


class IndependentExtensionControls(unittest.TestCase):
    def test_exact_review_commit_and_historical_runners_unchanged(self):
        for filename in ('compile7_source_cache.py', 'compile7_source_cache_test.py'):
            rel = 'tools/research/compile-performance/' + filename
            expected = subprocess.check_output(['git', 'show', COMMIT + ':' + rel], cwd=ROOT)
            self.assertEqual((TOOLS / filename).read_bytes(), expected)
        for filename in ('bench.py', 'rust_bench.py', 'runtime_bench.py'):
            rel = 'tools/research/compile-performance/' + filename
            expected = subprocess.check_output(['git', 'show', COMMIT + '^:' + rel], cwd=ROOT)
            self.assertEqual((TOOLS / filename).read_bytes(), expected)

    def test_validation_functions_are_retained(self):
        for wrapped, original, functions in (
            (zil_tests.bench, original_zil,
             ('one', 'execute', 'validate_rows', 'checked_preflight', 'measure', 'summarize')),
            (rust_tests.bench, original_rust,
             ('report_ok', 'native_ok', 'execute', 'validate_build_rows',
              'checked_preflight', 'measure', 'summarize')),
        ):
            for name in functions:
                with self.subTest(engine=wrapped.__name__, function=name):
                    self.assertEqual(getattr(wrapped, name).__code__, getattr(original, name).__code__)
                    self.assertIs(getattr(wrapped, name).__globals__, vars(wrapped))

    def test_load_extension_and_original_identity_guards(self):
        runner = zil_tests.bench
        with tempfile.TemporaryDirectory(prefix='cp31-independent-load-') as temporary:
            root = Path(temporary)
            (root / 'corpus').mkdir()
            source = root / 'corpus/main.zil'
            source.write_text('fn main()->i32=>0\n')
            plan = {'schema': runner.SCHEMA, 'compile7_source_cache': entry('zil'),
                    'runner_sha256': runner.sha(Path(runner.__file__)),
                    'dependencies_sha256': {}, 'binaries': {}, 'extra_tools': [],
                    'fixtures_sha256': runner.file_hashes(root / 'corpus')}
            path = root / 'plan.json'
            path.write_text(json.dumps(plan))
            self.assertEqual(runner.load(path, check_environment=False)[0], plan)
            with self.assertRaisesRegex(SystemExit, 'Runner/schema changed'):
                original_zil.load(path, check_environment=False)
            for field, value in (('extension_sha256', '0' * 64), ('engine', 'rust'),
                                 ('modes', {'A': True, 'B': True})):
                changed = copy.deepcopy(plan)
                changed['compile7_source_cache'][field] = value
                path.write_text(json.dumps(changed))
                with self.subTest(field=field), self.assertRaises(SystemExit):
                    runner.load(path, check_environment=False)
            changed = copy.deepcopy(plan)
            changed['runner_sha256'] = '0' * 64
            path.write_text(json.dumps(changed))
            with self.assertRaisesRegex(SystemExit, 'Runner/schema changed'):
                runner.load(path, check_environment=False)
            path.write_text(json.dumps(plan))
            source.write_text('fn main()->i32=>1\n')
            with self.assertRaisesRegex(SystemExit, 'Frozen fixture set changed'):
                runner.load(path, check_environment=False)

    def test_all_command_modes_only_remove_source_cache_pair(self):
        fixture = author.SourceCacheControls()
        work = Path('/tmp/cp31-not-executed')
        for engine, original in (('zil', original_zil), ('rust', original_rust)):
            for a in (False, True):
                for b in (False, True):
                    modes = {'A': a, 'B': b}
                    runner = extension.configure(engine, modes)
                    for cache_mode in ('cold', 'incremental'):
                        for phase in ('check', 'build'):
                            for arm in ('A', 'B'):
                                plan = fixture.plan(engine, modes)
                                plan['cache_mode'] = cache_mode
                                arg = {'arm': arm, 'phase': phase} if engine == 'zil' else arm
                                expected = original.command(plan, arg, work)
                                if not (engine == 'rust' and arm == 'A') and not modes[arm]:
                                    if '--cache-dir' in expected:
                                        index = expected.index('--cache-dir')
                                        del expected[index:index + 2]
                                with self.subTest(engine=engine, modes=modes, cache=cache_mode,
                                                  phase=phase, arm=arm):
                                    self.assertEqual(runner.command(plan, arg, work), expected)


if __name__ == '__main__':
    suite = unittest.TestSuite()
    for cls in (author.SourceCacheControls, ZilWrappedControls,
                RustWrappedControls, IndependentExtensionControls):
        suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(cls))
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)

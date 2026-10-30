#!/usr/bin/env python3
"""Independent synthetic CP24 controls; not compiler or performance evidence."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import types

ROOT = Path('/workspace/scratch/4ab361722127/zil')
COMMIT = 'c24cc08333dfb4ee710a7443414093f82841e0eb'
REL = 'tools/research/compile-performance/bench.py'
SPEC = importlib.util.spec_from_file_location('cp24_review_fixture', ROOT / 'tools/research/compile-performance/test_bench.py')
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)
BENCH = FIXTURE.bench
BASE = subprocess.check_output(['git', '-C', str(ROOT), 'show', COMMIT + '^:' + REL])
OLD = types.ModuleType('cp24_parent_bench')
OLD.__file__ = str(ROOT / REL)
exec(compile(BASE, OLD.__file__, 'exec'), OLD.__dict__)
RESULTS = []

def fixture():
    result = FIXTURE.RunnerControls()
    result.setUp()
    return result

def raw_change(test, rows, mutator, reseal=True, excerpt=False):
    path = test.root / 'logs' / rows[0]['stdout']
    report = json.loads(path.read_text())
    mutator(report)
    path.write_text(json.dumps(report))
    if reseal:
        rows[0]['stdout_sha256'] = BENCH.sha(path)
    if excerpt:
        for field in ('query_cache', 'parse_cache'):
            rows[0]['reported_' + field] = report.get(field)

test = fixture()
try:
    count = 0
    for phase in ('check', 'build'):
        for arm in ('A', 'B'):
            for objects in (False, True):
                seq = {**test.seq, 'phase': phase, 'arm': arm}
                plan = {**test.plan, 'object_cache': objects}
                old = OLD.command(plan, seq, test.root / 'sequence')
                for enabled in (None, False, True):
                    candidate = dict(plan)
                    if enabled is not None:
                        candidate['query_cache'] = enabled
                    actual = BENCH.command(candidate, seq, test.root / 'sequence')
                    expected = old + (['--query-cache', str(test.root / 'sequence/queries')] if enabled else [])
                    assert actual == expected, (phase, arm, objects, enabled)
                    count += 1
    RESULTS.append({'control': 'command_parent_parity_and_symmetric_opt_in', 'combinations': count, 'pass': True})
finally:
    test.tearDown()

SCENARIOS = [
    ('legacy_missing_fields', False, None, []),
    ('baseline_query_requested_without_parse_report', True, None, []),
    ('query_excerpt_tamper', True, lambda t, r: r[0].update(reported_query_cache={'requested': False}), [0]),
    ('parse_excerpt_tamper', True, lambda t, r: r[0].update(reported_parse_cache={'hits': 42}), [0]),
    ('raw_query_unsealed', True, lambda t, r: raw_change(t, r, lambda p: p['query_cache'].update(requested=False), reseal=False), [0]),
    ('raw_query_resealed_without_excerpt', True, lambda t, r: raw_change(t, r, lambda p: p['query_cache'].update(requested=False)), [0]),
    ('requested_false_resealed_and_excerpt_matched', True, lambda t, r: raw_change(t, r, lambda p: p['query_cache'].update(requested=False), excerpt=True), [0]),
    ('requested_integer_one_is_not_true', True, lambda t, r: raw_change(t, r, lambda p: p['query_cache'].update(requested=1), excerpt=True), [0]),
    ('query_report_missing_resealed', True, lambda t, r: raw_change(t, r, lambda p: p.pop('query_cache'), excerpt=True), [0]),
    ('query_report_wrong_type_resealed', True, lambda t, r: raw_change(t, r, lambda p: p.update(query_cache=[]), excerpt=True), [0]),
    ('parse_report_matched', True, lambda t, r: raw_change(t, r, lambda p: p.update(parse_cache={'requested': True, 'hits': 1, 'misses': 0}), excerpt=True), []),
    ('parse_raw_resealed_without_excerpt', True, lambda t, r: raw_change(t, r, lambda p: p.update(parse_cache={'hits': 42})), [0]),
    ('parse_raw_unsealed_even_with_excerpt', True, lambda t, r: raw_change(t, r, lambda p: p.update(parse_cache={'hits': 42}), reseal=False, excerpt=True), [0]),
    ('query_flag_missing', True, lambda t, r: r[0].update(argv=r[0]['argv'][:-2]), [0]),
    ('query_path_changed', True, lambda t, r: r[0]['argv'].__setitem__(-1, '/other/cache'), [0]),
]
for name, enabled, mutate, expected in SCENARIOS:
    test = fixture()
    try:
        if enabled:
            test.plan['query_cache'] = True
        rows = test.rows()
        if mutate:
            mutate(test, rows)
        complete, failures = BENCH.validate_rows(test.plan, test.root, test.root / 'records.jsonl', rows, test.plan['order'])
        assert complete and failures == expected, (name, complete, failures, expected)
        RESULTS.append({'control': name, 'rows': len(rows), 'failures': failures, 'pass': True})
    finally:
        test.tearDown()

test = fixture()
try:
    plan = {**test.plan, 'runner_sha256': BENCH.sha(ROOT / REL), 'dependencies_sha256': {},
            'binaries': {'python': BENCH.inventory(sys.executable)}, 'extra_tools': [],
            'environment': BENCH.environment(), 'host': BENCH.host()}
    test.plan_path.write_text(json.dumps(plan))
    loaded, _ = BENCH.load(test.plan_path, check_environment=False)
    assert 'query_cache' not in loaded
    RESULTS.append({'control': 'schema2_optional_field_with_matching_runner_hash_loads', 'pass': True})
    plan['runner_sha256'] = hashlib.sha256(BASE).hexdigest()
    test.plan_path.write_text(json.dumps(plan))
    try:
        BENCH.load(test.plan_path, check_environment=False)
    except SystemExit as error:
        assert 'Runner/schema changed' in str(error)
        RESULTS.append({'control': 'historical_runner_hash_remains_enforced', 'pass': True})
    else:
        raise AssertionError('Changed historical runner hash accepted')
finally:
    test.tearDown()

output = {'commit': COMMIT, 'scope': 'Synthetic runner controls; not compiler execution or performance evidence',
          'bench_sha256': BENCH.sha(ROOT / REL), 'controls': RESULTS, 'pass': True}
path = Path('/tmp/zil-review/CP24-probe-result.json')
path.write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps({'pass': True, 'controls': len(RESULTS), 'output': str(path)}))

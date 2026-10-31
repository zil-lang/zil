#!/usr/bin/env python3
"""Non-author read-only audit of actual CP31 full-CLI configuration records."""
import collections
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import random
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path('/workspace/scratch/c3607737969d')
MEASUREMENTS = ROOT / 'measurements'
PLAN = MEASUREMENTS / 'cp31-source-config-plan/plan.json'
PRE = MEASUREMENTS / 'cp31-source-config-preflight'
RUN = MEASUREMENTS / 'cp31-source-config-measure'
SUMMARY = MEASUREMENTS / 'cp31-source-config-summary.json'
OUTPUT = ROOT / 'review/CP31-SOURCE-CONFIG-INDEPENDENT-RECOMPUTE.json'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


plan = json.loads(PLAN.read_text())
summary = json.loads(SUMMARY.read_text())
extension_path = ROOT / 'zil/tools/research/compile-performance/compile7_source_cache.py'
spec = importlib.util.spec_from_file_location('review_cp31_extension', extension_path)
extension = importlib.util.module_from_spec(spec)
spec.loader.exec_module(extension)
runner = extension.configure('zil', {'A': True, 'B': False})
assert runner.load(PLAN, check_environment=False)[0] == plan
runner.checked_preflight(plan, PLAN.parent, PRE / 'result.json')
rows = [json.loads(line) for line in (RUN / 'records.jsonl').read_text().splitlines()]
pre_rows = [json.loads(line) for line in (PRE / 'records.jsonl').read_text().splitlines()]
complete, failures = runner.validate_rows(plan, PLAN.parent, RUN / 'records.jsonl', rows, plan['order'])
assert complete and failures == []
assert len(rows) == 240 and len(pre_rows) == 40
assert plan['phases'] == ['build'] and plan['blocks'] == 3
assert plan['source_identity']['A'] == plan['source_identity']['B']
assert plan['binaries']['A'] == plan['binaries']['B']
assert plan['compile7_source_cache']['modes'] == {'A': True, 'B': False}
assert plan['object_cache'] is True and plan['query_cache'] is True
assert plan['profile'] == 'O0' and plan['units'] == plan['jobs'] == 1
expected_cases = {'tree4x2': .05, 'tree32x8': .15, 'tree256x8': .30,
                  'tree1024x8': .30, 'retained': .20}
expected_requests = {'cold_process': .10, 'unchanged': .35,
                     'private_edit': .40, 'public_edit': .15}
assert {case['id']: case['weight'] for case in plan['cases']} == expected_cases
assert plan['request_weights'] == expected_requests and plan['selected_original_weight'] == 1.0
receipt = json.loads((RUN / 'complete.json').read_text())
assert receipt == {'plan_sha256': sha(PLAN), 'rows': 240, 'failures': [], 'pass': True,
                   'records_sha256': sha(RUN / 'records.jsonl'),
                   'preflight_sha256': sha(PRE / 'result.json')}
assert summary['plan_sha256'] == sha(PLAN)
assert summary['records_sha256'] == sha(RUN / 'records.jsonl')
assert summary['coverage_complete'] and summary['failures'] == []
assert summary['completion_receipt_matches'] and summary['preflight_matches']

counts = collections.Counter((r['block'], r['case'], r['request'], r['arm']) for r in rows)
assert len(counts) == 3 * 5 * 4 * 2 and set(counts.values()) == {2}
for block in range(3):
    for case in expected_cases:
        positions = sorted({(r['position'], r['arm']) for r in rows
                            if r['block'] == block and r['case'] == case})
        assert positions == list(enumerate('ABBA' if block % 2 == 0 else 'BAAB'))

native_counts = {}
stderr_counts = {}
query_states = collections.Counter()
output_hashes = collections.defaultdict(set)
llvm_hashes = collections.defaultdict(set)
for label, records, directory in (('preflight', pre_rows, PRE), ('measure', rows, RUN)):
    native = 0
    cli_nonempty = 0
    for row in records:
        assert row['pass'] is True and row['exit'] == 0 and row['timeout'] is False
        logs = directory / 'logs'
        for kind in ('stdout', 'stderr'):
            assert sha(logs / row[kind]) == row[kind + '_sha256']
        cli_nonempty += int((logs / row['stderr']).stat().st_size > 0)
        report = json.loads((logs / row['stdout']).read_text())
        assert report['status'] == 'Success' and report['verification_runs'] == 1
        assert report['native_codegen']['backend'] == 'llvm-api'
        assert report['query_cache']['requested'] is True
        assert row['executable_sha256'] == row['executable_after_sha256']
        assert row['executable_bytes'] > 0
        argv = row['argv']
        assert ('--cache-dir' in argv) == (row['arm'] == 'A')
        assert '--object-cache' in argv and '--query-cache' in argv
        assert row['argv'] == runner.command(plan, row, Path(row['work']))
        if label == 'measure':
            query_states[(row['arm'], row['request'], report['query_cache']['state'])] += 1
            output_hashes[(row['case'], row['snapshot'])].add(row['executable_sha256'])
            llvm_hashes[(row['case'], row['snapshot'])].add(row['llvm_sha256'])
        for check in row['checks']:
            native += 1
            assert check['matches'] is True and check['exit'] == 0 and check['timeout'] is False
            source = PLAN.parent / check['input']
            expected = PLAN.parent / check['expected']
            output, error = logs / check['stdout'], logs / check['stderr']
            assert sha(source) == check['input_sha256'] == check['input_after_sha256']
            assert sha(expected) == check['expected_stdout_sha256'] == sha(output) == check['stdout_sha256']
            assert output.read_bytes() == expected.read_bytes()
            assert error.stat().st_size == 0 and sha(error) == check['stderr_sha256']
    native_counts[label] = native
    stderr_counts[label] = cli_nonempty
assert native_counts == {'preflight': 128, 'measure': 768}
assert len(output_hashes) == 15 and all(len(values) == 1 for values in output_hashes.values())
assert len(llvm_hashes) == 15

# Independently implement the frozen aggregation contract; do not call legacy.aggregate.
block_costs = {}
for metric in ('wall', 'cpu'):
    for key in counts:
        values = [(r['wall_seconds'] if metric == 'wall' else r['cpu_user'] + r['cpu_system'])
                  for r in rows if (r['block'], r['case'], r['request'], r['arm']) == key]
        assert len(values) == 2 and all(math.isfinite(v) and v > 0 for v in values)
        block_costs[(metric,) + key] = statistics.mean(values)


def aggregate(metric, blocks):
    result = {'A': 0., 'B': 0., 'by_case': {}, 'by_request': {r: {'A': 0., 'B': 0.} for r in expected_requests}}
    for case, case_weight in expected_cases.items():
        group = {'A': 0., 'B': 0.}
        for request, request_weight in expected_requests.items():
            for arm in 'AB':
                absolute = statistics.median(block_costs[(metric, block, case, request, arm)] for block in blocks)
                group[arm] += request_weight * absolute
                result['by_request'][request][arm] += case_weight * absolute
                result[arm] += case_weight * request_weight * absolute
        group['B_over_A'] = group['B'] / group['A']
        result['by_case'][case] = group
    for values in result['by_request'].values():
        values['B_over_A'] = values['B'] / values['A']
    result['B_over_A'] = result['B'] / result['A']
    return result


def close(actual, expected):
    if isinstance(actual, dict):
        assert actual.keys() == expected.keys()
        for key in actual:
            close(actual[key], expected[key])
    else:
        assert math.isclose(actual, expected, rel_tol=1e-13, abs_tol=1e-14), (actual, expected)


costs = {metric: aggregate(metric, range(3)) for metric in ('wall', 'cpu')}
for metric, values in costs.items():
    close(values, summary['by_phase']['build'][metric])
rng = random.Random(plan['seed'])
samples = sorted(aggregate('wall', rng.choices(range(3), k=3))['B_over_A'] for _ in range(2000))
interval = [samples[49], samples[1949]]
for actual, expected in zip(interval, summary['by_phase']['build']['exploratory_paired_block_bootstrap95']):
    close(actual, expected)

result = {
    'reviewer': 'cp25_evidence',
    'verdict': 'Pass',
    'adoption': 'NoGo: disabling source candidates did not establish a complete-cost benefit',
    'scope': 'Read-only independent recomputation of actual full-CLI same-binary A=source-cache-on / B=off window; no new compilation',
    'frozen_inputs': {str(path): sha(path) for path in
                      (PLAN, PRE / 'result.json', PRE / 'records.jsonl', RUN / 'complete.json', RUN / 'records.jsonl', SUMMARY, extension_path)},
    'source_identity': plan['source_identity'], 'compiler_sha256': plan['binaries']['A']['sha256'],
    'profile': plan['profile'], 'units': plan['units'], 'jobs': plan['jobs'], 'blocks': 3,
    'case_weights': expected_cases, 'request_weights': expected_requests,
    'measurement_rows': 240, 'preflight_rows_unscored': 40,
    'native_output_checks': native_counts, 'nonempty_cli_stderr_logs': stderr_counts,
    'native_stderr_nonempty': 0, 'all_rows_verify_runs': 1,
    'arm_balance': 'Exactly two observations per arm/case/request/block; ABBA, BAAB, ABBA',
    'all_row_validation_passed': True, 'completion_and_preflight_receipts_match': True,
    'output_elf_hash_groups': len(output_hashes), 'output_llvm_hash_groups': len(llvm_hashes),
    'all_case_snapshot_elf_hashes_equal_across_arms_and_repeats': True,
    'llvm_sidecar_unique_digests_by_case_snapshot': {case + '/' + snapshot: len(values)
                                                   for (case, snapshot), values in sorted(llvm_hashes.items())},
    'query_cache_states': [{'arm': a, 'request': r, 'state': s, 'count': n}
                           for (a, r, s), n in sorted(query_states.items())],
    'aggregation': 'Mean of two observations within a block; median across frozen three blocks for each absolute arm/case/request cost; apply frozen case and request weights; divide summed absolute costs only after weighting',
    'recomputed_seconds': costs, 'exploratory_paired_block_bootstrap95': interval,
    'matches_published_summary': True,
    'limitations': [
        'Configuration comparison using the same ee72fd6 compiler, not CP25/29/30 implementation speed evidence',
        'Fresh CLI process per request; not a persistent session benchmark',
        'Shared host, unflushed OS page cache, three paired blocks; descriptive bootstrap only',
        'Native ELF/LLVM products were removed with disposable work; reviewed recorded before/after digests and all retained native output oracles, not a new rehash of deleted products',
        'LLVM sidecar digests differ across sequences; the text serializer contains a verified-fingerprint comment. LLVM text equality is not claimed; final ELF digests agree for each case/snapshot',
        'Current static tool/source/fixture identity verified; original runtime environment is retained in the plan, not claimed re-executed',
        'No formal product gate or whole-language acceptance'],
    'audit_script_sha256': sha(__file__),
    'initial_audit_run': 'The initial audit script incorrectly required LLVM sidecar byte identity across sequences. That extra assertion failed and was removed; the measurement and raw evidence were unchanged. The first audit log is retained.',
}
OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'verdict': result['verdict'], 'rows': 240, 'native': native_counts,
                  'wall': costs['wall']['B_over_A'], 'cpu': costs['cpu']['B_over_A'],
                  'stderr': stderr_counts, 'report': str(OUTPUT)}, indent=2))

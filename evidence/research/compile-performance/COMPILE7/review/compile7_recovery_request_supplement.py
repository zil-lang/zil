#!/usr/bin/env python3
"""Read-only seven-request statistics supplement to a completed audited window."""
import argparse
import hashlib
import importlib.util
import itertools
import json
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path('/workspace/scratch/c3607737969d')
AUDIT = ROOT / 'review/compile7_session_independent_recompute.py'
AUDIT_SHA = '4ae659fe9996473ba42673f3820f6bd19c64e53f29b954017d2541151996da01'

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-result', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not args.out.exists()
    assert sha(AUDIT) == AUDIT_SHA
    spec = importlib.util.spec_from_file_location('unchanged_request_statistics', AUDIT)
    audit = importlib.util.module_from_spec(spec); spec.loader.exec_module(audit)
    source = audit.read(args.source_result)
    assert source['verdict'] == 'Pass'
    assert source['case_weights'] == audit.CASES
    assert source['four_request_weights'] == audit.CORE_REQUESTS
    results = {}
    for name, window in source['windows'].items():
        rows_path = Path(window['root']) / 'measure/rows.jsonl'
        assert sha(rows_path) == window['records_sha256']
        rows = [json.loads(line) for line in rows_path.read_text().splitlines()]
        assert len(rows) == 200
        costs = {}
        for capacity, case, request, arm, block in itertools.product(
                audit.CAPACITIES, audit.CASES, audit.REQUESTS, 'AB', range(5)):
            observations = [audit.value(row, request) for row in rows
                            if (row['capacity'], row['case'], row['arm'], row['block'])
                            == (capacity, case, arm, block)]
            assert len(observations) == 2
            costs[capacity, case, request, arm, block] = statistics.mean(observations)
        capacities = {}
        for capacity in audit.CAPACITIES:
            requests = {}
            for request in audit.REQUESTS:
                components = [(weight, *[[costs[capacity, case, request, arm, b]
                                        for b in range(5)] for arm in 'AB'])
                              for case, weight in audit.CASES.items()]
                requests[request] = audit.weighted_result(components, 'ms')
                for case, arm in itertools.product(audit.CASES, 'AB'):
                    actual = statistics.median(costs[capacity, case, request, arm, b] for b in range(5))
                    expected = window['capacities'][capacity]['by_case'][case]['request_median_compile_ms'][request][arm]
                    audit.near(actual, expected)
                if request in audit.CORE_REQUESTS:
                    assert requests[request] == window['capacities'][capacity]['case_weighted_core_requests'][request]
            capacities[capacity] = requests
        results[name] = capacities
    result = {'reviewer': 'cp25_evidence', 'verdict': 'Pass',
              'scope': 'Supplementary fixed-case-weight statistics for all seven requests, including restore/private_again/restore_again; no new timing or execution',
              'source_result': str(args.source_result), 'source_result_sha256': sha(args.source_result),
              'original_records_sha256': {k: v['records_sha256'] for k, v in source['windows'].items()},
              'case_weights': audit.CASES, 'fixed_primary_weights_unchanged': audit.CORE_REQUESTS,
              'aggregation_and_bootstrap_unchanged': source['aggregation'],
              'bootstrap': source['bootstrap'], 'requests': results,
              'audit_script_sha256': sha(__file__)}
    args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({'verdict': 'Pass', 'recovery_requests': {name: {cap: {
        req: {'B_over_A': values[req]['B_over_A'], 'exploratory_CI95': values[req]['exploratory_paired_block_bootstrap95']}
        for req in audit.REQUESTS[4:]} for cap, values in caps.items()} for name, caps in results.items()}}, indent=2))

if __name__ == '__main__': main()

#!/usr/bin/env python3
"""Read-only current push consumer receipt checks; never infer missing work as zero.

CLI accepts one report JSON or resident response JSON. This is instrumentation
validation, not a performance, semantic or independent-authority oracle. The
matched runner separately checks current Verify, diagnostics and native output.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path

# Explicit experiment pins, not evidence of the remote branch's current head.
MAIN = '42f0396f36b2a36860ecb55371d436e4327877a4'
LEGACY_MAIN = '99035931d358add0eaed6d4535035c38413701dd'
BASELINE_PINS = (MAIN, LEGACY_MAIN)
LLVM_RAW = 'complete diagnostic LLVM bytes'
LLVM_PUSH = 'complete diagnostic LLVM bytes except the strict second-line verified-fingerprint hex64 value; raw bytes and hashes retained'
CANDIDATE_FLAGS = frozenset(('--producer-reuse', '--reactive-queries', '--push-reuse'))
FIELDS = {
    'types_push_work': ('context_rebuilds', 'sparse_batches', 'body_checks',
                        'position_rebindings', 'retained_functions', 'receipt_tree_updates'),
    'initialization': ('product_admission_functions_checked', 'product_function_queries_executed',
                       'product_function_queries_reused', 'product_global_obligations_executed',
                       'product_global_obligations_reused', 'product_plan_rows_replaced'),
    'verification_work': ('full_receipts_reused', 'function_proofs_checked', 'function_proofs_reused',
                          'instruction_proofs_reused', 'empty_borrow_proofs_checked',
                          'empty_borrow_proofs_reused', 'empty_borrow_values_avoided',
                          'raw_shape_functions_checked', 'raw_shape_functions_reused',
                          'raw_inventory_functions_checked', 'raw_inventory_functions_reused',
                          'raw_shape_functions_aggregated', 'raw_shape_ranges_reused',
                          'raw_result_modes_checked', 'raw_result_modes_reused',
                          'resource_inventory_functions_checked', 'resource_inventory_functions_reused',
                          'resource_inventory_ranges_reused', 'foreign_functions_selected',
                          'foreign_functions_not_visited', 'region_functions_checked',
                          'region_functions_reused', 'lexical_cleanup_functions_checked',
                          'lexical_cleanup_functions_reused'),
    'lowering_work': ('functions_checked', 'layout_checks', 'functions_reused', 'full_checks',
                      'budget_replays', 'context_checks', 'context_check_ns'),
}


def candidate_flags(flags):
    if (not isinstance(flags, list) or any(not isinstance(flag, str) for flag in flags)
            or not set(flags) <= CANDIDATE_FLAGS or len(flags) != len(set(flags))):
        raise ValueError('candidate extras must be unique research switches; no profile/backend/cache/budget overrides')
    return flags


def baseline_pin(actual, requested):
    if requested not in BASELINE_PINS or actual != requested:
        raise ValueError('baseline source differs from the explicit supported experiment pin')


def command_flags(command, arm, extras):
    expected = candidate_flags(extras) if arm == 'candidate' else []
    actual = [arg for arg in command if arg in CANDIDATE_FLAGS]
    if actual != expected:
        raise ValueError('actual research flags differ from the manifest for ' + arm)
    if '--profile-compilation' in command or '--incremental-o2' in command:
        raise ValueError('profile or alternate backend path in formal wall run')


def llvm_comparison_sha256(data):
    """Only the fixed second-line identity value differs between proof domains."""
    prefix = re.match(rb'\A; ZIL verified LLVM module\n; verified-fingerprint: ([0-9a-f]{64})\n', data)
    if prefix is None:
        raise ValueError('LLVM diagnostic lacks the exact second-line verified-fingerprint hex64 header')
    start, end = prefix.span(1)
    return hashlib.sha256(data[:start] + b'0' * 64 + data[end:]).hexdigest()


def inspect_report(report, *, mode, cold=False, profiled=False):
    """Validate available receipts and require them when current stages ran.

    Complete frontend/candidate hits legitimately lack Types/Analyze receipts.
    Failed Types currently publishes no partial work receipt, so None is unknown,
    not zero. Events and execution assertions remain separate from field shape.
    Cold means the *first* request of a fresh block, never a later cold preparation.
    """
    if mode not in ('check', 'o2'):
        raise ValueError('unsupported push measurement mode')
    events = report['query_events']
    executed = {event['phase'] for event in events if event['disposition'] == 'executed'}
    front = report.get('frontend_stats')
    if front is None:
        front = {}
    if not isinstance(front, dict):
        raise ValueError('frontend_stats must be an object or null')
    groups = {name: (front.get(name) if name == 'initialization' else report.get(name)) for name in FIELDS}
    present = {name: name in (front if name == 'initialization' else report) for name in FIELDS}
    for name, fields in FIELDS.items():
        value = groups[name]
        if value is None:
            continue
        if not isinstance(value, dict):
            raise ValueError(name + ' must be an object or null')
        for field in fields:
            number = value.get(field)
            if type(number) is not int or number < 0:
                raise ValueError(name + '.' + field + ' missing or not a nonnegative integer')
        if name == 'lowering_work' and value.get('cost_profiled') is not profiled:
            raise ValueError('lowering_work.cost_profiled differs from the declared timing mode')
    failed_types = report['status'] != 'Success' and report['phase'] == 'types'
    if type(report['verification_runs']) is not int or report['verification_runs'] < 0:
        raise ValueError('verification_runs must be a nonnegative integer')
    required = {
        'types_push_work': 'types' in executed and not failed_types,
        'initialization': 'analyze' in executed,
        'verification_work': report['verification_runs'] > 0,
        'lowering_work': 'lower' in executed or report['phase'] == 'lower'
                         or (mode == 'o2' and report['status'] == 'Success'),
    }
    for name, needed in required.items():
        if needed and groups[name] is None:
            raise ValueError(name + ' unavailable despite current stage execution')
    if report['status'] == 'Success':
        if report['verification_runs'] != 1 or sum(event == {'phase': 'verify', 'disposition': 'executed'} for event in events) != 1:
            raise ValueError('successful push request lacks one current independent Verify invocation')
        if mode == 'o2' and 'lower' not in executed:
            raise ValueError('successful native request lacks current Lower execution')
    if cold:
        if report['status'] != 'Success' or not {'types', 'analyze', 'verify'} <= executed:
            raise ValueError('first cold request did not execute current pipeline stages')
        if groups['types_push_work']['body_checks'] <= 0:
            raise ValueError('first cold request did not check any bodies')
        analysis = groups['initialization']
        if analysis['product_function_queries_executed'] + analysis['product_global_obligations_executed'] <= 0:
            raise ValueError('first cold request did not execute actual Analyze consumers')
        verify = groups['verification_work']
        if verify['raw_shape_functions_checked'] + verify['raw_shape_functions_aggregated'] <= 0:
            raise ValueError('first cold request did not perform independent raw shape checks')
        if mode == 'o2' and groups['lowering_work']['functions_checked'] <= 0:
            raise ValueError('first cold request did not admit any Lower functions')
    return {'groups': groups, 'fields_present': present, 'required_by_execution': required,
            'unavailable_is_not_zero': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--mode', choices=('check', 'o2'), required=True)
    parser.add_argument('--cold', action='store_true', help='only the first request in a fresh block')
    parser.add_argument('--profiled', action='store_true', help='diagnostic window only, never formal wall comparison')
    args = parser.parse_args()
    try:
        value = json.loads(args.report.read_text())
        result = inspect_report(value.get('report', value), mode=args.mode, cold=args.cold, profiled=args.profiled)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({'valid': False, 'error': str(error)}))
        return 1
    print(json.dumps({'valid': True, **result}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

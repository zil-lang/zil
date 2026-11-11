#!/usr/bin/env python3
"""Validate one complete A/B/C run and retain every matched contrast.

No pooling across workload sizes, profiled/unprofiled runs, or compiler SHAs.
One block diagnoses correctness/work only; intervals are exploratory means.
"""
import argparse
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path
import random

from native_abc_measure import ARMS, CONTRASTS, ORDERS, row_valid, validate_cost_profile
from native_abc_workloads import oracle as workload_oracle
from native_measure import pair_validation, transition, validate_response
from native_summarize import (arm_details, describe, interval, json_safe, metrics, number)
from native_workloads import inputs as workload_inputs


def validate_sidecars(directory, row, name):
    worker = directory / f'block-{row["block"]}-{row["arm"]}'
    response = json.loads((worker / f'{name}.response.json').read_text())
    if response != row.get('resident'):
        raise ValueError('response sidecar differs from the complete row response')
    samples = json.loads((worker / f'{name}.memory-samples.json').read_text())
    if not isinstance(samples, list) or not samples:
        raise ValueError('memory sample sidecar must be a nonempty list')
    previous = -1
    for sample in samples:
        if not isinstance(sample, dict):
            raise ValueError('invalid memory sample record')
        offset = sample.get('offset_ms')
        if not number(offset) or offset < 0 or offset < previous:
            raise ValueError('memory sample offsets must be finite, nonnegative and ordered')
        previous = offset
        if type(sample.get('processes')) is not int or sample['processes'] <= 0:
            raise ValueError('memory sample must observe a positive process count')
        rss = sample.get('tree_rss_sum_kib')
        if not number(rss) or rss <= 0:
            raise ValueError('memory sample RSS must be finite and positive')
    if type(row.get('memory_sample_count')) is not int or len(samples) != row['memory_sample_count']:
        raise ValueError('memory sample count differs from its sidecar')
    if max(sample['tree_rss_sum_kib'] for sample in samples) != row.get('tree_peak_sampled_rss_sum_kib'):
        raise ValueError('sampled peak RSS differs from the sidecar maximum')


def validate_native_oracle(row, args):
    native = row.get('native', [])
    if not isinstance(native, list) or any(not isinstance(item, dict) for item in native):
        raise ValueError('invalid native outcome records')
    if args['mode'] == 'check' or row.get('request') == 'error':
        if native:
            raise ValueError('check/rejected request unexpectedly claims native executions')
        return
    expected_inputs = workload_inputs(args['case'])
    if [item.get('stdin_hex') for item in native] != [data.hex() for data in expected_inputs]:
        raise ValueError('native inputs are incomplete, duplicated or out of workload order')
    for data, item in zip(expected_inputs, native):
        expected = workload_oracle(args['case'], data, row['request'],
                                   args['modules'], args['functions'], args['steps'])
        if (item.get('valid') is not True or item.get('expected') != expected
                or item.get('actual') != expected):
            raise ValueError('saved native outcome differs from the recomputed independent oracle')


def summarize(directory, seed=20261111, repetitions=20000):
    errors = []
    result = {'run': str(directory.resolve()), 'errors': errors,
              'performance_acceptance': 'not_decided_by_tool'}
    try:
        manifest = json.loads((directory / 'manifest.json').read_text())
        summary = json.loads((directory / 'summary.json').read_text())
        rows = [json.loads(line) for line in (directory / 'rows.jsonl').read_text().splitlines() if line.strip()]
    except (OSError, ValueError) as error:
        result['complete_valid_evidence'] = False
        errors.append(f'incomplete/unreadable run: {error}')
        return result
    args = manifest['arguments']
    blocks, labels = args['blocks'], manifest['requests']
    result.update(identity={key: manifest.get(key) for key in ['source_sha', 'binary', 'script_sha256']},
                  configuration=args, purpose=manifest.get('purpose'))
    if type(blocks) is not int or blocks < 1 or not labels or len(set(labels)) != len(labels):
        raise ValueError('invalid declared block/request matrix')
    if manifest.get('schema') != 'native-abc-v1' or manifest.get('arms') != ARMS:
        errors.append('missing/invalid A/B/C arm definitions')
    expected_purpose = 'profiled_cost_diagnostic' if args['profile_compilation'] else 'unprofiled_wall'
    if manifest.get('purpose') != expected_purpose:
        errors.append('profile purpose contradicts command arguments')
    scripts = {'native_abc_measure.py', 'native_abc_summarize.py', 'native_abc_workloads.py',
               'native_measure.py', 'native_workloads.py', 'native_summarize.py'}
    if set(manifest.get('script_sha256', {})) != scripts:
        errors.append('manifest does not bind the complete runner/oracle/summarizer scripts')
    for name, expected in manifest.get('script_sha256', {}).items():
        try:
            if hashlib.sha256((directory / name).read_bytes()).hexdigest() != expected:
                errors.append(f'archived script hash differs: {name}')
        except OSError as error:
            errors.append(str(error))
    if summary.get('complete') is not True or summary.get('fatal_error') is not None:
        errors.append('runner did not complete successfully')
    if summary.get('binary_sha256_after') != manifest['binary']['sha256']:
        errors.append('compiler binary changed during the run')
    observed = {'rows': len(rows), 'measured_rows': sum(row.get('measured') is True for row in rows),
                'invalid_rows': sum(row.get('valid') is not True for row in rows),
                'native_executions': sum(len(row.get('native', [])) for row in rows)}
    for name, value in observed.items():
        if summary.get(name) != value:
            errors.append(f'raw/summary mismatch: {name}')
    counts = Counter()
    rejected = []
    expected_sequence, cold_succeeded = [], False
    for index, label in enumerate(labels):
        parent = transition(args['case'], label)
        if label == 'restore' and not cold_succeeded:
            expected_sequence.append((index, 'cold', False, None))
            cold_succeeded = True
        if parent is not None:
            expected_sequence.append((index, parent, False, None))
            cold_succeeded |= parent == 'cold'
        expected_sequence.append((index, label, True, parent))
        cold_succeeded |= label == 'cold'
    for block in range(blocks):
        for arm, flags in ARMS.items():
            sequence = [(row.get('request_index'), row.get('request'), row.get('measured'), row.get('transition_from'))
                        for row in rows if row.get('block') == block and row.get('arm') == arm]
            if sequence != expected_sequence:
                errors.append(f'preparation/measured history differs: block={block} arm={arm}')
            try:
                startup = json.loads((directory / f'block-{block}-{arm}' / 'startup.json').read_text())
                command = startup['command']
                if command[0] != manifest['binary']['path']:
                    errors.append('arm uses a different compiler binary path')
                for flag in ['--producer-reuse', '--reactive-queries', '--profile-compilation']:
                    expected = int(flag in flags or (flag == '--profile-compilation' and args['profile_compilation']))
                    if command.count(flag) != expected:
                        errors.append(f'actual startup flag matrix differs: {arm} {flag}')
            except (OSError, KeyError, ValueError) as error:
                errors.append(f'missing/invalid startup command: {error}')
    for row in rows:
        before = len(errors)
        identity = (row.get('block'), row.get('arm'))
        counts[identity] += 1
        if type(row.get('measured')) is not bool or row.get('valid') is not True or row.get('paired_semantics_equal') is not True:
            errors.append(f'invalid row: {identity} request={row.get("request")}')
        response = row.get('resident', {})
        try:
            validate_response(response, counts[identity])
            validate_cost_profile(response['report'], args['profile_compilation'])
            if not row_valid(response['report'], row['request']):
                errors.append('invalid current compiler result or Verify count')
        except (KeyError, TypeError, ValueError) as error:
            errors.append(f'invalid full response: {error}')
        for field in ['wall_ms', 'cpu_seconds', 'own_cpu_seconds', 'tree_peak_sampled_rss_sum_kib', 'memory_sample_count']:
            if not number(row.get(field)) or row[field] < 0:
                errors.append(f'missing/invalid metric: {field}')
        if row.get('memory_sample_count', 0) <= 0 or row.get('tree_peak_sampled_rss_sum_kib', 0) <= 0:
            errors.append('missing sampled memory')
        name = f'{row.get("request_index")}-' + ('' if row.get('measured') else 'prepare-') + str(row.get('request'))
        try:
            validate_sidecars(directory, row, name)
        except (OSError, KeyError, TypeError, ValueError) as error:
            errors.append(f'missing/invalid per-request sidecars: {error}')
        try:
            source = json.loads((directory / f'source-{row.get("block")}-{name}.json').read_text())
            if source.get('identity_sha256') != row.get('source_identity_sha256') or source.get('changed_files') != row.get('changed_files'):
                errors.append('row differs from archived source metadata')
            archived = directory / 'snapshots' / row['request']
            files = {str(path.relative_to(archived)): hashlib.sha256(path.read_bytes()).hexdigest() for path in archived.rglob('*.zil')}
            identity = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
            if files != source.get('file_hashes') or identity != source.get('identity_sha256'):
                errors.append('source snapshot bytes differ from archived metadata')
        except (OSError, KeyError, ValueError) as error:
            errors.append(f'missing/invalid source evidence: {error}')
        try:
            validate_native_oracle(row, args)
        except (KeyError, TypeError, ValueError) as error:
            errors.append(f'missing/invalid independent native oracle: {error}')
        if len(errors) != before:
            rejected.append({'errors': errors[before:], 'row': row})
    measured = [row for row in rows if row.get('measured') is True]
    if len(measured) != blocks * len(labels) * 3:
        errors.append('incomplete measured A/B/C matrix')
    matrix = {}
    for label in labels:
        matrix[label] = []
        for block in range(blocks):
            group = [row for row in measured if row.get('request') == label and row.get('block') == block]
            if len(group) != 3 or {row.get('arm') for row in group} != set(ARMS):
                errors.append(f'missing/duplicate triplet: {label} block={block}')
                continue
            indexed = {row['arm']: row for row in group}
            if len({row.get('source_identity_sha256') for row in group}) != 1:
                errors.append(f'triplet source mismatch: {label} block={block}')
            if not all(pair_validation(list(pair)) for pair in itertools.combinations(group, 2)):
                errors.append(f'triplet public semantic mismatch: {label} block={block}')
            if any([item.get('stdin_hex') for item in indexed[arm].get('native', [])] !=
                   [item.get('stdin_hex') for item in indexed['A'].get('native', [])] for arm in ARMS):
                errors.append('triplet native input mismatch')
            order = ORDERS[(block + labels.index(label)) % len(ORDERS)]
            if any(indexed[arm].get('position') != position for position, arm in enumerate(order)):
                errors.append('triplet execution order differs from declared schedule')
            matrix[label].append(indexed)
    result.update(counts=observed, rejected_rows=rejected, complete_valid_evidence=not errors)
    result['order_balance'] = {'complete_six_block_cycles': blocks // 6,
                               'remaining_blocks': blocks % 6,
                               'fully_balanced': blocks % 6 == 0}
    result['preparations'] = {arm: arm_details([row for row in rows if row.get('measured') is False and row.get('arm') == arm]) for arm in ARMS}
    if errors:
        result['scenarios'] = {label: {'observed_triplets': len(group), 'expected_triplets': blocks} for label, group in matrix.items()}
        return result
    rng = random.Random(seed)
    draws = [[rng.randrange(blocks) for _ in range(blocks)] for _ in range(repetitions)]
    result['interpretation'] = ('Profiled diagnostic times are not unprofiled wall evidence.' if args['profile_compilation'] else
                                'Unprofiled matched request times; no performance acceptance is inferred.')
    result['bootstrap'] = {'seed': seed, 'replicates': repetitions, 'unit': 'matched block',
                           'single_block_work_diagnostic_only': blocks == 1,
                           'stable_p95_estimate': False}
    result['scenarios'] = {}
    for label, groups in matrix.items():
        scenario = {'arms': {arm: {'wall_ms': describe(group[arm]['wall_ms'] for group in groups),
                                   'details': arm_details([group[arm] for group in groups]),
                                   'complete_report_numeric_fields': metrics([group[arm] for group in groups], lambda row: row['resident']['report'])}
                              for arm in ARMS}, 'contrasts': {}}
        for first, second in CONTRASTS:
            saved = [group[first]['wall_ms'] - group[second]['wall_ms'] for group in groups]
            contrast = {'paired_saved_ms': describe(saved),
                        'paired_second_over_first': describe(group[second]['wall_ms'] / group[first]['wall_ms'] for group in groups),
                        'positive_means_second_faster': True}
            if blocks > 1:
                contrast['paired_mean_saved_ms_ci'] = interval(saved, draws)
            scenario['contrasts'][first + '_minus_' + second] = contrast
        result['scenarios'][label] = scenario
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--seed', type=int, default=20261111)
    parser.add_argument('--bootstrap', type=int, default=20000)
    args = parser.parse_args()
    if args.bootstrap < 1:
        parser.error('bootstrap must be positive')
    result = summarize(args.run, args.seed, args.bootstrap)
    print(json.dumps(json_safe(result), ensure_ascii=False, indent=2, allow_nan=False))
    return int(not result['complete_valid_evidence'])


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""In-memory negative tests on completed real window metadata only.

Run AFTER timing, for example:
  python3 -B tools/research/incremental-demand/test_cohort_window_analysis.py --check CHECK_WINDOW --o2 O2_WINDOW

No raw files are modified, no compiler/native program runs, and no artifact
blob is read or rehashed. This tests the analyzer's pure structural helper,
not pins, physical artifacts, or the original runner's oracle algorithms.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True


def load_analyzer(path):
    spec = importlib.util.spec_from_file_location('strict_analysis_under_test', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_metadata(path):
    """Read small JSON/JSONL receipts, never archive/blob references."""
    def read(relative):
        return json.loads((path/relative).read_text())
    completion = read('completion.json')
    if completion.get('valid') is not True:
        raise ValueError('tests require a genuinely completed window: '+str(path))
    manifest = read('manifest.json')
    return {'mode': manifest['arguments']['mode'], 'manifest': manifest,
            'completion': completion,
            'rows': [json.loads(line) for line in (path/'rows.jsonl').read_text().splitlines()],
            'pairs': [(f.relative_to(path).as_posix(), json.loads(f.read_text()))
                      for f in sorted(path.rglob('pair-validation.json'))]}


def first_row(data, *, measured=None, request=None, block=0, native=False):
    return next(row for row in data['rows']
                if row['block'] == block and (measured is None or row['measured'] is measured)
                and (request is None or row['request'] == request)
                and (not native or bool(row.get('native'))))


def first_pair(data, *, native=False):
    return next(pair for _, pair in data['pairs'] if not native or pair['executable_equal'] is True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--analyzer', type=Path,
                        default=Path(__file__).with_name('cohort_window_analysis.py'))
    parser.add_argument('--check', type=Path, required=True)
    parser.add_argument('--o2', type=Path, required=True)
    args = parser.parse_args()
    analyzer = load_analyzer(args.analyzer)
    check, o2 = load_metadata(args.check), load_metadata(args.o2)
    assert check['mode'] == 'check' and o2['mode'] == 'o2'
    passed = []

    def validate(data):
        return analyzer.validate_quality_structure(data['manifest'], data['completion'],
                                                   data['rows'], data['pairs'], data['mode'])

    def reject(name, base, mutate):
        data = copy.deepcopy(base)
        mutate(data)
        try:
            validate(data)
        except (ValueError, KeyError, TypeError):
            passed.append(name)
        else:
            raise AssertionError('bad metadata accepted: '+name)

    # Positive controls are the exact completed check and OLD completed O2
    # metadata. The running six-block O2 window is never an input here.
    for name, data in (('real-completed-check', check), ('real-completed-old-o2', o2)):
        counts = validate(data)
        assert len(counts) == data['manifest']['arguments']['blocks']
        for count in counts:
            assert tuple(count[k] for k in ('rows', 'measured', 'preparations', 'pair_records',
                                            'native_outcomes', 'full_ELF_equal_pairs')) == (
                51, 27, 24, 17, 540 if data['mode'] == 'o2' else 0, 15 if data['mode'] == 'o2' else 0)
        passed.append(name)
    reordered_pairs = copy.deepcopy(check)
    reordered_pairs['pairs'].reverse()
    validate(reordered_pairs)
    passed.append('receipt-enumeration-order-is-not-request-identity')

    # Astra's three original total-preserving counterexamples.
    reject('preparation-moved-across-blocks', check,
           lambda d: first_row(d, measured=False).__setitem__('block', 1))

    def move_pair(data):
        rel, pair = data['pairs'][0]
        data['pairs'][0] = ('block-01/requests/99-cold-parent/pair-validation.json', pair)
    reject('pair-moved-to-unused-other-block-path', o2, move_pair)

    def move_native(data, block):
        source = first_row(data, native=True)
        destination = next(row for row in data['rows'] if row is not source
                           and row['block'] == block and row.get('native'))
        destination['native'].append(source['native'].pop())
    reject('native-11-and-13-same-block', o2, lambda d: move_native(d, 0))
    reject('native-11-and-13-cross-block', o2, lambda d: move_native(d, 1))

    for name, value in (('bool', False), ('float', 0.0), ('outside', 6), ('negative', -1)):
        reject('block-'+name, check, lambda d, v=value: d['rows'][0].__setitem__('block', v))
    for field, value in (('request_index', False), ('request_index', 0.0),
                         ('request_index', 9), ('measured', 1), ('measured', 'false'),
                         ('target_request', 'private'), ('request', 'private'), ('arm', 'other')):
        reject('row-identity-'+field+'-'+str(value), check,
               lambda d, f=field, v=value: d['rows'][0].__setitem__(f, v))
    reject('duplicate-parent-replaces-missing-parent', check,
           lambda d: d['rows'].__setitem__(4, copy.deepcopy(d['rows'][3])))
    reject('extra-row', check, lambda d: d['rows'].append(copy.deepcopy(d['rows'][0])))
    reject('missing-row', check, lambda d: d['rows'].pop())
    reject('non-bool-row-valid', check, lambda d: d['rows'][0].__setitem__('valid', 1))
    reject('parent-marked-as-target-transition', check,
           lambda d: first_row(d, measured=False).__setitem__('transition_from', 'cold'))
    reject('recovery-must-follow-error', check,
           lambda d: first_row(d, measured=True, request='recovery').__setitem__('transition_from', 'cold'))
    reject('restore-must-follow-private', check,
           lambda d: first_row(d, measured=True, request='restore').__setitem__('transition_from', 'cold'))
    reject('missing-transition-field', check,
           lambda d: first_row(d, measured=False).pop('transition_from'))
    reject('wrong-directory-block', check,
           lambda d: d['rows'][0].__setitem__('directory', d['rows'][0]['directory'].replace('block-00', 'block-01')))
    reject('absolute-directory', check,
           lambda d: d['rows'][0].__setitem__('directory', '/'+d['rows'][0]['directory']))
    reject('parent-directory-for-measured-row', check,
           lambda d: d['rows'][0].__setitem__('directory', d['rows'][0]['directory'].replace('measured', 'parent')))
    reject('arm-position-bool', check, lambda d: d['rows'][0].__setitem__('position', False))
    reject('wrong-arm-order', check, lambda d: d['rows'][0]['order'].reverse())
    reject('execution-order-target-before-parent', check,
           lambda d: d['rows'].__setitem__(slice(3, 9), d['rows'][6:9]+d['rows'][3:6]))

    reject('duplicate-pair-replaces-missing-pair', check,
           lambda d: d['pairs'].__setitem__(1, copy.deepcopy(d['pairs'][0])))
    reject('extra-pair', check, lambda d: d['pairs'].append(copy.deepcopy(d['pairs'][0])))
    reject('missing-pair', check, lambda d: d['pairs'].pop())
    for field, value in (('request', 'other'), ('measured', False), ('measured', 1),
                         ('valid', 1), ('semantics_equal', False), ('schema', 'other')):
        # Pick a measured receipt explicitly; filesystem sorting places
        # cold-measured first, but identity must not depend on that accident.
        def change_pair(data, field=field, value=value):
            pair = next(pair for _, pair in data['pairs'] if pair['measured'] is True)
            pair[field] = value
        reject('pair-identity-'+field+'-'+str(value), check, change_pair)

    for value in (1, True, 3):
        reject('native-repetitions-'+str(value), o2,
               lambda d, v=value: d['manifest']['arguments'].__setitem__('native_repetitions', v))
    reject('fixed-block-count-disagrees', check,
           lambda d: d['manifest']['fixed'].__setitem__('blocks', 2))
    reject('fixed-request-order-changed', check, lambda d: d['manifest']['fixed']['requests'].reverse())
    reject('completion-total-changed', check, lambda d: d['completion'].__setitem__('preparations', 0))

    def move_into_error(data):
        source = first_row(data, native=True)
        first_row(data, request='error')['native'] = [source['native'].pop()]
    reject('error-native-one-success-eleven-total-unchanged', o2, move_into_error)
    reject('check-cannot-claim-native-outcomes', check,
           lambda d: d['rows'][0].__setitem__('native', copy.deepcopy(first_row(o2, native=True)['native'])))
    reject('native-list-shape', o2, lambda d: first_row(d, native=True).__setitem__('native', {}))
    reject('duplicate-native-outcome-with-count-twelve', o2,
           lambda d: first_row(d, native=True)['native'].__setitem__(1, copy.deepcopy(first_row(d, native=True)['native'][0])))
    for field, value in (('stdin_hex', '42'), ('directory', 'wrong/native-00-00'), ('valid', 1)):
        reject('native-identity-'+field, o2,
               lambda d, f=field, v=value: first_row(d, native=True)['native'][0].__setitem__(f, v))
    reject('native-actual-disagrees-with-original-expected', o2,
           lambda d: first_row(d, native=True)['native'][0]['actual'].__setitem__('exit', 99))
    for field in ('executable_equal', 'aggregate_zil_function_exports_equal',
                  'native_oracles_valid', 'physical_zil_units_valid'):
        reject('successful-pair-'+field, o2,
               lambda d, f=field: first_pair(d, native=True).__setitem__(f, False))

    def shift_elf_count(data):
        success = first_pair(data, native=True)
        error = next(pair for _, pair in data['pairs'] if pair['request'] == 'error')
        success['executable_equal'], error['executable_equal'] = None, True
    reject('elf-count-preserved-by-crediting-error', o2, shift_elf_count)
    reject('pair-elf-reference-disagrees', o2,
           lambda d: next(row for row in d['rows'] if row['arm'] == 'candidate' and row.get('native'))['executable'].__setitem__('sha256', '0'*64))
    reject('physical-cgu16-count', o2,
           lambda d: next(row for row in d['rows'] if row['arm'] == 'candidate' and row.get('native'))['inventory'].__setitem__('object_count', 15))

    # The two REAL inputs intentionally have different block counts. They may
    # pass separately, but cannot constitute one accepted paired window.
    check_summary = {'mode': 'check', 'blocks': check['manifest']['arguments']['blocks']}
    o2_summary = {'mode': 'o2', 'blocks': o2['manifest']['arguments']['blocks']}
    if check_summary['blocks'] != o2_summary['blocks']:
        try:
            analyzer.require_matching_blocks(check_summary, o2_summary)
        except ValueError:
            passed.append('real-check-six-old-o2-two-cannot-be-combined')
        else:
            raise AssertionError('mixed completed window block counts accepted')
    for blocks in (check_summary['blocks'], o2_summary['blocks']):
        analyzer.require_matching_blocks({'mode': 'check', 'blocks': blocks}, {'mode': 'o2', 'blocks': blocks})
    passed.append('matching-two-and-six-block-summary-controls')
    print(json.dumps({'valid': True, 'passed': len(passed), 'cases': passed,
                      'metadata_inputs': [str(args.check), str(args.o2)],
                      'scope': 'Pure metadata helpers; no blob reads/hash, writes, build or native execution.'}, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

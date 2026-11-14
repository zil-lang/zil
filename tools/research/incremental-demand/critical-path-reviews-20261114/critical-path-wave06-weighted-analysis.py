#!/usr/bin/env python3
"""Read completed wave06 evidence; never run compilers, Git, benchmarks or polls.

Default/--prepare only writes the plan; --analyze is for the batch-done notice.
--self-check checks arithmetic with in-memory examples, without reading windows.
Only the two named coverage outputs beside this script are written.
"""
import argparse
import collections
import datetime
import hashlib
import json
import math
from pathlib import Path
import statistics

SOURCE = '789f879f249c7f45a5c84ce062be30f9c0985740'
PRIOR_SOURCE = '5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d'
MAIN = '9170bb4210554392e36eb7528596612596ebef80'
TOOL = 'ad81486b976634e71bf1c61544d0fd28a783fbae'
ARMS = ('baseline', 'candidate', 'rust')
LABELS = (
    'large-project-large2500-06-o2',
    'large-project-large1000-edits-06-o2',
)
METRICS = ('physical_bytes', 'logical_bytes', 'files', 'unique_files',
           'directories', 'symlinks', 'path_logical_bytes')
LIMITS = (
    'Synthetic scalar fixtures, not industrial projects or industrial edit frequencies.',
    'Per-window current main/candidate/Rust only; no cross-window grand speedup.',
    'Two blocks unless the actual manifest states otherwise; no confidence claim.',
    'Resident ZIL response wall versus fresh Cargo/rustc; startup scopes differ.',
    'OS caches are not flushed; outside-timer disk/native scans may warm caches.',
    'Disk is recorded boundary storage, not transient build peak, RSS or SDK size.',
    'Physical bytes include recorded directory/symlink blocks; logical bytes use recorded inode deduplication.',
    'Missing counters are unknown, not zero; nested/parallel timings are not additive wall partitions.',
    'Emission reused_functions are current-request post-O2 body reuses, not incremental function cache hits.',
    'normalized_functions is an observed normalization count, not an extra emission reuse or MC hit; missing is unknown.',
    'Fallback receipts can include discarded-attempt work; retain fallback flags and ordinary object costs.',
    '5105 capacity observations never enter workflow sums, timing ratios or disk advantage comparisons.',
    'No compiler/native quality gates are rerun by this analyzer; it checks recorded evidence consistency.',
)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def number(value, positive=False):
    require(type(value) in (int, float) and math.isfinite(value), 'nonfinite/non-numeric observation')
    require(value > 0 if positive else value >= 0, 'out-of-range observation')
    return value


def read_json(path, identities):
    before = path.stat()
    raw = path.read_bytes()
    after = path.stat()
    require((before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_ino, after.st_size, after.st_mtime_ns), f'evidence changed: {path}')
    identities[str(path)] = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    return json.loads(raw)


def flatten_numbers(value, prefix=''):
    result = {}
    if isinstance(value, dict):
        for key, child in value.items():
            name = prefix + '.' + key if prefix else key
            if isinstance(child, dict):
                result.update(flatten_numbers(child, name))
            elif type(child) in (int, float):
                result[name] = number(child)
    return result


def report_from(row):
    report = (row.get('resident') or {}).get('report')
    return report if isinstance(report, dict) else {}


def emission(report):
    native = report.get('native_codegen') or {}
    objects = []
    for index, obj in enumerate(native.get('object_reports') or []):
        if isinstance(obj, str):
            obj = json.loads(obj)
        require(isinstance(obj, dict), 'object receipt must be an object')
        objects.append({
            'unit': index, 'input_kind': obj.get('input_kind'),
            'emission_reuse': obj.get('emission_reuse'),
            'native_products': {key: (obj.get('native_products') or {}).get(key)
                                for key in ('available', 'machine_function_executions',
                                            'machine_function_reuses', 'fresh_context_fallbacks',
                                            'failed_replays', 'fallback_ns')},
            'timings_ns': {key: obj[key] for key in ('preflight_ns', 'builder_ns', 'parse_ns',
                           'optimize_ns', 'emit_ns', 'total_ns') if key in obj},
            'verify_before_after': obj.get('verify_before_after'),
            'llvm_verification_this_request': obj.get('llvm_verification_this_request'),
            'patch_fallback': obj.get('patch_fallback'),
        })
    receipts = [obj['emission_reuse'] for obj in objects if obj['emission_reuse'] is not None]
    fields = ('considered_functions', 'distinct_bodies', 'reused_functions', 'normalized_functions', 'selection_ns',
              'machine_assembly_ns', 'reconstruction_ns', 'assembly_ns')
    sums = {}
    for key in fields:
        values = [receipt[key] for receipt in receipts if key in receipt]
        sums[key] = sum(number(v) for v in values) if values else None
        if key == 'normalized_functions' and len(values) != len(receipts):
            sums[key] = None
    fallback = [r['fallback'] for r in receipts if type(r.get('fallback')) is bool]
    complete_reuse_counts = receipts and all(type(r.get('fallback')) is bool
        and type(r.get('reused_functions')) is int for r in receipts)
    effective_reuses = sum(number(r['reused_functions']) for r in receipts if not r['fallback']) \
        if complete_reuse_counts else None
    return {'object_receipts': objects, 'emission_receipt_count': len(receipts),
            'emission_sums': sums,
            'normalized_functions_observed_units': sum('normalized_functions' in r for r in receipts),
            'successful_reused_functions': effective_reuses,
            'fallback_units': sum(fallback) if fallback else None,
            'fallback_observed_units': len(fallback),
            'backend_fallback': native.get('fallback'),
            'final_artifact': native.get('final_artifact'),
            'actual_units': native.get('actual_units'),
            'timings_ns': native.get('timings'),
            'native_product_selection': report.get('native_product_selection')}


def slim_row(row, line):
    report = report_from(row)
    stats = {}
    for key in ('phase_timings_ns', 'driver_timings_ns', 'backend_work', 'lowering_work',
                'frontend_stats', 'semantic_cache_publication', 'function_cache_publication',
                'verification_work'):
        stats.update(flatten_numbers(report.get(key), key))
    return {key: row.get(key) for key in ('block', 'index', 'request', 'arm', 'valid',
            'mode', 'directory', 'source_identity_sha256', 'failure', 'raw_response',
            'disk_before', 'disk_after')} | {
        'line': line, 'measurement': row.get('measurement'),
        'status': report.get('status'), 'phase': report.get('phase'),
        'diagnostics': report.get('diagnostics'), 'verification_runs': report.get('verification_runs'),
        'verify_events': sum(e.get('phase') == 'verify' and e.get('disposition') == 'executed'
                             for e in report.get('query_events', [])),
        'stats': stats, 'emission': emission(report),
        'source': {key: (row.get('source') or {}).get(key)
                   for key in ('identity_sha256', 'source_files', 'source_bytes')},
    }


def read_rows(path, identities):
    before = path.stat()
    digest = hashlib.sha256()
    result = []
    with path.open('rb') as stream:
        for line, raw in enumerate(stream, 1):
            digest.update(raw)
            if raw.strip():
                row = json.loads(raw)
                if not report_from(row) and row.get('raw_response'):
                    response_path = Path(row['raw_response'])
                    if not response_path.is_absolute():
                        response_path = path.parent / response_path
                    require(response_path.resolve().is_relative_to(path.parent.resolve()),
                            'raw response escaped this evidence window')
                    if response_path.is_file():
                        response = read_json(response_path, identities)
                        if isinstance(response, dict) and isinstance(response.get('report'), dict):
                            row['resident'] = response
                result.append(slim_row(row, line))
    after = path.stat()
    require((before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_ino, after.st_size, after.st_mtime_ns), f'rows changed: {path}')
    identities[str(path)] = {'bytes': after.st_size, 'sha256': digest.hexdigest()}
    return result


def arm_total(snapshot, arm):
    require(isinstance(snapshot, dict) and isinstance(snapshot.get('categories'), dict),
            'missing disk snapshot/category table')
    categories = snapshot['categories']
    require(all(name.split('.', 1)[0] in ARMS for name in categories), 'unknown disk arm')
    for metric in METRICS:
        require(sum(number(c[metric]) for c in categories.values()) == snapshot['total'][metric],
                'recorded disk total disagrees with category sum: ' + metric)
    return {metric: sum(c[metric] for name, c in categories.items() if name.startswith(arm + '.'))
            for metric in METRICS}


def group(request):
    return 'private' if request.startswith('private-') else request if request in ('cold', 'broad') else 'other'


def weighted(comparisons):
    result = {}
    for name in ('cold', 'broad', 'private', 'other'):
        selected = [r for r in comparisons if group(r['request']) == name]
        totals = {arm: sum(r['median_ms'][arm] for r in selected) for arm in ARMS}
        result[name] = {'frequency': len(selected), 'total_ms': totals,
                        'mean_per_request_ms': {arm: totals[arm] / len(selected) if selected else None
                                                for arm in ARMS},
                        'saved_vs_main_ms': totals['baseline'] - totals['candidate'],
                        'saved_vs_rust_ms': totals['rust'] - totals['candidate']}
    return result


def disk_analysis(rows, blocks, private_count):
    peaks = {arm: {} for arm in ARMS}
    for row in rows:
        for moment in ('disk_before', 'disk_after'):
            for arm in ARMS:
                totals = arm_total(row[moment], arm)
                for metric, value in totals.items():
                    if metric not in peaks[arm] or value > peaks[arm][metric]['value']:
                        peaks[arm][metric] = {
                            'value': value, 'block': row['block'], 'request': row['request'],
                            'observing_arm': row['arm'], 'moment': moment, 'row_line': row['line'],
                            'categories': {k: v[metric] for k, v in row[moment]['categories'].items()
                                           if k.startswith(arm + '.')},
                        }
    growth = []
    for block in range(blocks):
        item = {'block': block, 'private_count': private_count, 'arms': {}}
        for arm in ARMS:
            selected = {r['request']: r for r in rows if r['block'] == block and r['arm'] == arm}
            cold = arm_total(selected['cold']['disk_after'], arm)
            edited = arm_total(selected['unchanged-after-edits']['disk_after'], arm)
            restored = arm_total(selected['restore']['disk_after'], arm)
            restarted = arm_total(selected['warm-restart']['disk_after'], arm)
            epochs = []
            previous = cold
            for name in sorted(n for n in selected if n.startswith('private-')):
                current = arm_total(selected[name]['disk_after'], arm)
                epochs.append({'request': name, 'total': current,
                               'delta_previous': {k: current[k] - previous[k] for k in METRICS}})
                previous = current
            item['arms'][arm] = {
                'cold': cold, 'after_private_edits': edited,
                'growth_private': {k: edited[k] - cold[k] for k in METRICS},
                'mean_growth_bytes_per_private_edit': {
                    k: (edited[k] - cold[k]) / private_count if private_count else None
                    for k in ('physical_bytes', 'logical_bytes')},
                'warm_restart_delta': {k: restarted[k] - restored[k] for k in METRICS},
                'epochs': epochs,
            }
        growth.append(item)
    return {'peaks': peaks, 'by_block': growth,
            'median_growth_private': {arm: {metric: statistics.median(
                b['arms'][arm]['growth_private'][metric] for b in growth) for metric in METRICS}
                for arm in ARMS}, 'transient_peak': None}


def stats_analysis(rows, names):
    result = {}
    for name in names:
        arms = {}
        for arm in ARMS[:2]:
            selected = [r for r in rows if r['request'] == name and r['arm'] == arm]
            keys = sorted({key for r in selected for key in r['stats']})
            arms[arm] = {
                'median_observed_fields': {key: statistics.median(r['stats'][key] for r in selected)
                    if all(key in r['stats'] for r in selected) else None for key in keys},
                'by_block': [{key: r[key] for key in ('block', 'line', 'directory', 'status', 'phase',
                             'diagnostics', 'verification_runs', 'verify_events', 'stats', 'emission')}
                             for r in selected],
            }
        result[name] = arms
    return result


def emission_totals(rows):
    """Count observed receipt events, never distinct functions or missing data."""
    result = {}
    for arm in ARMS[:2]:
        selected = [r for r in rows if r['arm'] == arm]
        blocks = []
        for block in sorted({r['block'] for r in selected}):
            current = [r for r in selected if r['block'] == block]
            observed = [r['emission'] for r in current if r['emission']['emission_receipt_count']]
            def observed_sum(key):
                values = [e[key] for e in observed if e[key] is not None]
                return sum(values) if values else None
            blocks.append({
                'block': block, 'requests': len(current),
                'requests_with_emission_receipts': len(observed),
                'requests_without_emission_receipts': len(current) - len(observed),
                'emission_receipt_units': sum(e['emission_receipt_count'] for e in observed),
                'successful_reused_function_events': observed_sum('successful_reused_functions'),
                'fallback_units': observed_sum('fallback_units'),
                'observed_emission_sums': {key: sum(e['emission_sums'][key] for e in observed)
                    if observed and all(e['emission_sums'][key] is not None for e in observed) else None
                    for key in ('considered_functions', 'distinct_bodies', 'reused_functions', 'normalized_functions',
                                'selection_ns', 'machine_assembly_ns', 'reconstruction_ns', 'assembly_ns')},
            })
        result[arm] = {
            'by_block': blocks,
            'all_recorded_blocks_reuse_events': sum(b['successful_reused_function_events'] for b in blocks)
                if blocks and all(b['successful_reused_function_events'] is not None for b in blocks) else None,
            'scope': 'sum of observed non-fallback receipt events; repeated functions/requests/blocks count again; not unique functions',
        }
    return result


def identity_check(manifest, identities):
    pins = manifest['pins']
    require(pins['candidate']['source'] == SOURCE, 'unexpected candidate source')
    require(pins['baseline']['source'] == MAIN, 'unexpected current main source')
    require(pins['tool_source'] == TOOL and manifest['tool_source']['commit'] == TOOL,
            'unexpected frozen harness source')
    for arm in ARMS[:2]:
        pin = pins[arm]
        source = manifest['sources'][arm]
        require(source['commit'] == pin['source'] and source['checkout_commit'] == pin['source'],
                arm + ' source binding mismatch')
        require(source['status'] == '', arm + ' source was dirty')
        require(manifest['binaries'][arm]['sha256'] == pin['binary_sha256'], 'binary pin mismatch')
        receipt = read_json(Path(pin['build_receipt']), identities)
        require(receipt['source'] == pin['source'] and receipt['tree'] == source['tree']
                and receipt['sha256'] == pin['binary_sha256']
                and receipt['exit_code'] == 0 and receipt.get('command'), 'build receipt mismatch')
    return {key: manifest.get(key) for key in ('pins', 'sources', 'binaries', 'tool_source', 'tools',
            'implementation_sha256', 'explicit_environment', 'budgets', 'quality_policy',
            'measurement_scope', 'disk_scope', 'native_profile_archive')}


def analyze_window(root):
    identities = {}
    result = {'path': str(root), 'state': 'pending', 'comparisons': [], 'disk': None,
              'totals_ms': None, 'contributions': None, 'issues': [], 'identities': identities}
    if not all((root / f).is_file() for f in ('manifest.json', 'summary.json', 'completion.json', 'rows.jsonl')):
        result['issues'].append('No complete evidence set; no partial comparison.')
        return result
    try:
        manifest = read_json(root / 'manifest.json', identities)
        summary = read_json(root / 'summary.json', identities)
        complete = read_json(root / 'completion.json', identities)
        rows = read_rows(root / 'rows.jsonl', identities)
        result['binding'] = identity_check(manifest, identities)
        result['arguments'] = manifest['arguments']
        require(manifest.get('generated_only') is False, 'generation is not measurement')
        expected_mode = root.name.rsplit('-', 1)[-1]
        expected_graph = 'chain' if '-chain-' in root.name else 'diamond' if '-diamond-' in root.name else 'wide'
        expected_epochs = 32 if '1000-edits' in root.name else 8
        require(manifest['arguments']['mode'] == expected_mode
                and manifest['arguments']['graph'] == expected_graph
                and manifest['arguments']['epochs'] == expected_epochs,
                'window label/history arguments mismatch')
        result['history'] = manifest['history']
        fixture_path = root / 'requests/block-00/000-cold/equivalence.json'
        if fixture_path.is_file():
            result['fixture'] = read_json(fixture_path, identities)
        result['raw_completion'] = complete
        result['failure'] = summary.get('failure') or manifest.get('failure')
        result['actual_rows'] = len(rows)
        result['expected_rows'] = summary.get('expected_rows')
        result['emission_totals'] = emission_totals(rows)
        result['observed_request_details'] = stats_analysis(rows, [h['name'] for h in manifest['history']])
        if not all(x.get('valid') is True for x in (manifest, summary, complete)):
            require(summary.get('comparisons') == [], 'failed runner emitted comparisons')
            result['state'] = 'failed'
            result['failed_quality_records'] = []
            for block, index, name in sorted({(r['block'], r['index'], r['request']) for r in rows}):
                quality_path = root / 'requests' / f'block-{block:02d}' / f'{index:03d}-{name}' / 'pair-validation.json'
                if quality_path.is_file():
                    quality = read_json(quality_path, identities)
                    if quality.get('valid') is not True:
                        result['failed_quality_records'].append(quality)
            result['failed_rows'] = [{k: r[k] for k in ('block', 'request', 'arm', 'valid',
                                    'status', 'phase', 'diagnostics', 'failure', 'raw_response')}
                                    for r in rows if not r['valid']]
            return result
        names = [h['name'] for h in manifest['history']]
        blocks = manifest['arguments']['blocks']
        require(sum(n.startswith('private-') for n in names) == expected_epochs, 'private frequency mismatch')
        require(len(names) == len(set(names)), 'duplicate history names')
        expected = blocks * len(names) * len(ARMS)
        require(len(rows) == summary['actual_rows'] == complete['rows'] == expected
                == summary['expected_rows'] == complete['expected_rows'], 'incomplete row grid')
        require(complete['pairs'] == blocks * len(names), 'incomplete quality pair count')
        grid = {(r['block'], r['request'], r['arm']): r for r in rows}
        require(len(grid) == expected, 'duplicate row key')
        for block in range(blocks):
            for index, name in enumerate(names):
                triplet = [grid[(block, name, arm)] for arm in ARMS]
                require(all(r['valid'] is True and r['index'] == index for r in triplet), 'invalid row')
                require(len({r['source_identity_sha256'] for r in triplet}) == 1, 'fixture triplet mismatch')
                quality_path = root / 'requests' / f'block-{block:02d}' / f'{index:03d}-{name}' / 'pair-validation.json'
                quality = read_json(quality_path, identities)
                require(quality['valid'] is True and quality['request'] == name
                        and quality['block'] == block and quality['index'] == index, 'quality record mismatch')
                if manifest['arguments']['mode'] == 'o2' and name != 'error':
                    require(all(quality.get(k) is True for k in ('semantics_equal', 'executable_equal',
                        'native_oracles_valid', 'physical_zil_units_valid', 'aggregate_zil_function_exports_equal')),
                        'native/ELF/physical unit quality did not pass')
                for row in triplet:
                    number(row['measurement']['wall_ms'], positive=True)
                    if row['arm'] != 'rust' and name != 'error':
                        require(row['status'] == 'Success' and row['verification_runs'] >= 1
                                and row['verify_events'] >= 1, 'successful ZIL request lacks current Verify')
                    elif row['arm'] != 'rust':
                        require(row['status'] not in (None, 'Success', 'ResourceLimit', 'Unsupported',
                                                      'Cancelled', 'InfraError'), 'unexpected error-request result')
        comparisons = []
        for name in names:
            samples = {arm: [grid[(block, name, arm)]['measurement']['wall_ms']
                             for block in range(blocks)] for arm in ARMS}
            comparisons.append({'request': name, 'samples_ms': samples,
                                'median_ms': {arm: statistics.median(v) for arm, v in samples.items()}})
        recorded = {r['request']: r for r in summary['comparisons']}
        require(set(recorded) == set(names), 'summary request coverage mismatch')
        for comparison in comparisons:
            for arm in ARMS:
                require(math.isclose(comparison['median_ms'][arm],
                    recorded[comparison['request']]['median_ms'][arm], rel_tol=1e-12), 'median mismatch')
        disk = disk_analysis(rows, blocks, sum(n.startswith('private-') for n in names))
        totals = {arm: sum(r['median_ms'][arm] for r in comparisons) for arm in ARMS}
        result.update(state='valid', comparisons=comparisons, disk=disk, totals_ms=totals,
                      contributions=weighted(comparisons),
                      saved_vs_main_ms=totals['baseline'] - totals['candidate'],
                      saved_vs_rust_ms=totals['rust'] - totals['candidate'])
    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
        result.update(state='evidence-invalid', comparisons=[], disk=None, totals_ms=None,
                      contributions=None)
        result['issues'].append(type(error).__name__ + ': ' + str(error))
    return result


def capacity(root):
    identities = {}
    out = {'path': str(root), 'state': 'pending', 'scope': 'capacity only; not a matched performance window',
           'comparisons': [], 'formal_ratio': None, 'identities': identities}
    if not all((root / f).is_file() for f in ('binding.json', 'fixture.json', 'summary.json')):
        return out
    try:
        binding = read_json(root / 'binding.json', identities)
        fixture = read_json(root / 'fixture.json', identities)
        summary = read_json(root / 'summary.json', identities)
        require(binding['candidate_source'] == SOURCE and summary['source'] == SOURCE, 'capacity source mismatch')
        require(binding['baseline']['source'] == MAIN, 'capacity main mismatch')
        out['binding'] = binding
        out['fixture'] = fixture
        out['complete'] = summary.get('complete')
        out['state'] = 'recorded' if summary.get('complete') is True else 'incomplete'
        out['rows'] = [{k: v for k, v in row.items() if k not in ('source', 'report', 'response', 'runtime', 'native')}
                       | {'diagnostics': row.get('diagnostics'), 'native_observations': len(row.get('native') or []),
                          'historical_only': row.get('reused_receipt') is True}
                      for row in summary.get('rows', [])]
        out['independent_observation_counts'] = {
            'candidate_source_requests': sum(r.get('arm') == 'candidate' for r in summary.get('rows', [])),
            'candidate_requests_with_V1_and_inventory': sum(
                r.get('arm') == 'candidate' and r.get('status') == 'Success'
                and r.get('verification_runs') == 1 and r.get('current_verify_events') == 1
                and r.get('source_inventory_matches_all_102') is True
                for r in summary.get('rows', [])),
            'native_valid_observations_by_arm': {
                arm: sum(obs.get('valid') is True and obs.get('actual') == obs.get('expected')
                         for r in summary.get('rows', []) if r.get('arm') == arm
                         for obs in (r.get('native') or [])) for arm in ARMS},
        }
        for raw_row, row in zip(summary.get('rows', []), out['rows']):
            if raw_row.get('response'):
                response_path = Path(raw_row['response'])
                if not response_path.is_absolute():
                    response_path = root / response_path
                require(response_path.resolve().is_relative_to(root.resolve()),
                        'capacity response escaped this window')
                if response_path.is_file():
                    response = read_json(response_path, identities)
                    report = response.get('report') or {}
                    row['current_emission'] = emission(report)
                    row['phase_timings_ns'] = report.get('phase_timings_ns')
                    row['backend_work'] = report.get('backend_work')
        for k in ('protocol_valid', 'diagnostic_success', 'candidate_full_source_verify_native_success',
                  'rust_check_receipt_and_o2_success'):
            out[k] = summary.get(k)
    except (OSError, ValueError, KeyError, TypeError) as error:
        out.update(state='evidence-invalid', error=str(error))
    return out


def current_gates(runs_root, windows):
    root = runs_root / ('critical-path-wave23-' + SOURCE[:12])
    identities = {}
    completion = read_json(root / 'completion.json', identities)
    release = read_json(root / 'release.json', identities)
    require(completion.get('source') == SOURCE and completion.get('all_gates_valid') is True,
            'current gate completion/source mismatch')
    require(release.get('source') == SOURCE and release.get('valid') is True
            and release.get('exit_code') == 0, 'current release/source mismatch')
    for window in windows.values():
        if window.get('binding'):
            require(window['binding']['pins']['candidate']['binary_sha256'] == release['sha256'],
                    'measured binary does not match gate23 release')
    return {'path': str(root), 'identities': identities, 'completion': completion,
            'release': {k: release.get(k) for k in ('source', 'tree', 'valid', 'exit_code', 'sha256', 'binary', 'command')},
            'scope': 'recorded gate/release receipts and measured pin consistency; no gates/compiler rerun'}


def prior_context():
    """Explicit historical references; never rebind check04 to the 06 source."""
    root = Path('/workspace/zil-recovery')
    identities = {}
    prior = read_json(root / 'critical-path-wave04-weighted-coverage.json', identities)
    require(prior['candidate_source'] == PRIOR_SOURCE and prior['state'] == 'analyzed',
            'wave04 archive source/state mismatch')
    for suffix in ('coverage.md', 'analysis.py'):
        path = root / ('critical-path-wave04-weighted-' + suffix)
        with path.open('rb') as stream:
            identities[str(path)] = {'bytes': path.stat().st_size,
                                    'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}
    checks = {}
    for label in ('large-project-large1000-edits-04-check', 'large-project-large2500-04-check'):
        old = prior['windows'][label]
        require(old['state'] == 'valid', 'referenced check04 was not valid')
        checks[label] = {key: old[key] for key in ('path', 'state', 'totals_ms', 'comparisons',
                                                  'contributions', 'disk', 'identities')}
        checks[label].update(measured_candidate_source=PRIOR_SOURCE,
                             reuse_kind='explicit historical check04 reference, not a measurement of 789f879f')
    out = {
        'candidate_source': PRIOR_SOURCE, 'archive_identities': identities,
        'reused_check04': checks,
        'source_difference': {
            'from': PRIOR_SOURCE, 'to': SOURCE,
            'changed_production_paths': ['crates/zil-llvm-api/src/native/emission.rs', 'crates/zil-llvm-api/src/lib.rs'],
            'observation_field_change': 'LLVM API lib.rs only adds normalized_functions field and JSON serialization',
            'unchanged_paths': 'driver, Source/Expand, transport, Types, Analyze, Verify and root Cargo inputs',
            'nonproduction_changes': 'archived reports/evidence also added; excluded from compiler source-equivalence claim',
            'verification': 'preparation-time read-only git diff --name-only over crates/Cargo inputs and lib.rs diff; no Git called by analyzer',
            'reuse_basis': 'user-authorized check04 code-path equivalence reference; native emission plus observation field changed; no binary/timing equivalence claim',
        },
        'native_counter_reference': {name: {
            'fixture': w.get('fixture'), 'details': w.get('observed_request_details'),
            'state': w['state'], 'source': PRIOR_SOURCE}
            for name, w in prior['windows'].items()
            if name in {label.replace('-06-', '-04-') for label in LABELS}},
    }
    return out


def counter_deltas(windows, prior):
    result = {}
    for name, window in windows.items():
        old = prior['native_counter_reference'].get(name.replace('-06-', '-04-'))
        if not old or 'observed_request_details' not in window:
            continue
        fields = ('named_declarations', 'function_definitions', 'shape', 'graph', 'coefficient_sha256')
        require(all((old.get('fixture') or {}).get(k) == (window.get('fixture') or {}).get(k)
                    for k in fields), '04/06 fixture mismatch for counter comparison')
        matched = []
        for request, arms in window['observed_request_details'].items():
            previous = {r['block']: r for r in (old['details'] or {}).get(request, {})
                        .get('candidate', {}).get('by_block', [])}
            for row in arms['candidate']['by_block']:
                if row['block'] not in previous:
                    continue
                old_count = previous[row['block']]['emission']['successful_reused_functions']
                new_count = row['emission']['successful_reused_functions']
                matched.append({'request': request, 'block': row['block'],
                                'source04': PRIOR_SOURCE, 'source06': SOURCE,
                                'reuse_events04': old_count, 'reuse_events06': new_count,
                                'added_events': new_count - old_count
                                    if old_count is not None and new_count is not None else None,
                                'fallback_units06': row['emission']['fallback_units'],
                                'scope': 'receipt count delta only; not cross-window timing attribution'})
        result[name] = matched
    return result


def render(data):
    lines = ['# Critical-path wave06 weighted coverage', '',
             f"State: **{data['state']}**. Candidate `{SOURCE}`; main `{MAIN}`; frozen tool `{TOOL}`.", '',
             'Sum the median wall time of each actual request; positive savings = reference − candidate. '
             '`broad` means the named broad request; error/recovery remain separate requests in other. '
             'No cross-window aggregate or industrial frequency assumption.', '',
             'Predeclared expectation (not an observation): large2500 cold normalized_functions=0 while cheap-shape/local-name-profile '
             'selection may retain 2,092 cold reuse events. This expectation never fills observations '
             'or authorizes extra MC hits; actual receipt counts decide.', '',
             '| Window | State | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |',
             '| --- | --- | ---: | ---: | ---: | ---: | ---: |']
    for label, w in data['windows'].items():
        if w['state'] == 'valid':
            t = w['totals_ms']
            values = [t[a] / 1000 for a in ARMS] + [w['saved_vs_main_ms'] / 1000, w['saved_vs_rust_ms'] / 1000]
            cells = ' | '.join(f'{v:.3f}' for v in values)
        else:
            cells = '— | — | — | — | —'
        lines.append(f"| {label} | {w['state']} | {cells} |")
    if data['state'] == 'analyzed':
        valid = [w for w in data['windows'].values() if w['state'] == 'valid']
        failed = [w for w in data['windows'].values() if w['state'] == 'failed']
        rows = sum(w.get('actual_rows', 0) for w in data['windows'].values())
        lines += ['', f"**Final wave06 evidence: {len(valid)}/{len(data['windows'])} valid windows, {len(failed)} failed; "
                  f"{rows} recorded matrix rows.** Results belong only to `{SOURCE}`. "
                  'Check04 retains its original 5a source; any later code receives none of these measurements.', '']
        lines += ['Matrix composition: ' + '; '.join(
            f"{label}: {w.get('actual_rows')} recorded/{w.get('expected_rows')} expected ({w['state']})"
            for label, w in data['windows'].items()) + '. Capacity06 is counted separately.', '']
        gate = data['current_gates']
        lines += [f"Gate23 recorded all_gates_valid={gate['completion']['all_gates_valid']}; "
                  f"release and both measured window pins bind binary `{gate['release']['sha256']}`. "
                  'This is a receipt consistency check, not an additional compiler execution.', '']
        if valid and all(w['saved_vs_main_ms'] > 0 and w['saved_vs_rust_ms'] > 0 for w in valid):
            lines += [f'All {len(valid)} valid workflow totals favor the candidate over main and Rust. '
                      'Signed request losses remain below; workflow gains do not imply every cold/broad request wins.', '']
        known_deltas = [row['added_events'] for matches in data.get('native_count_deltas_vs04', {}).values()
                        for row in matches if row['added_events'] is not None]
        if known_deltas and all(delta == 0 for delta in known_deltas):
            lines += [f'**Negative result: all {len(known_deltas)} matched request/block reuse-count deltas '
                      'against 04 are zero.** The cost-filter candidate added no observed reuse events in these inputs. '
                      'This is a counter result, not a causal performance comparison between the two windows.', '']
        for label, window in data['windows'].items():
            if window['state'] == 'valid' and window['saved_vs_rust_ms'] < 0:
                lines += [f"- {label}: candidate workflow is {-window['saved_vs_rust_ms']/1000:.3f}s slower than Rust."]
        lines += ['']
        lines += ['## Observed native emission reuse totals', '',
                  'These are dynamic events, not unique source functions. Counts sum only actual non-fallback '
                  'emission receipts; missing/cached receipts are not filled with zero. '
                  'Failed-window counts are diagnostic observations, not accepted performance results.', '',
                  '| Window | Block | Receipt requests | Receipt units | Reuse events | Normalized functions | Fallback units |',
                  '| --- | ---: | ---: | ---: | ---: | ---: | ---: |']
        for label, w in data['windows'].items():
            for b in w.get('emission_totals', {}).get('candidate', {}).get('by_block', []):
                if not b['emission_receipt_units']:
                    continue
                lines.append(f"| {label} | {b['block']} | {b['requests_with_emission_receipts']} | "
                             f"{b['emission_receipt_units']} | {b['successful_reused_function_events']} | "
                             f"{b['observed_emission_sums']['normalized_functions']} | {b['fallback_units']} |")
        lines += ['', 'Cold/broad normalization must not be generalized to all edits. '
                  'Every nonzero observed normalization count is retained here; failed-window rows are diagnostic only.', '',
                  '| Window | Request | Block | Normalized functions | Reused events | Selection ms (sum of unit receipts) |',
                  '| --- | --- | ---: | ---: | ---: | ---: |']
        for label, w in data['windows'].items():
            for request, arms in w.get('observed_request_details', {}).items():
                for row in arms['candidate']['by_block']:
                    e = row['emission']
                    count = e['emission_sums']['normalized_functions']
                    if count:
                        selection = e['emission_sums']['selection_ns']
                        selection_text = f'{selection / 1e6:.3f}' if selection is not None else 'unknown'
                        lines.append(f"| {label} | {request} | {row['block']} | {count} | "
                                     f"{e['successful_reused_functions']} | {selection_text} |")
        lines += ['', 'Totals and phase costs above/below come only from actual 06 receipts. '
                  'No 04 native counts or first-cold observations are filled into missing 06 requests.', '',
                  '## Cost-filter candidate: observed added hits versus wave04', '',
                  'Count deltas retain both source identities and do not establish causal time savings. '
                  'A zero delta is an explicit negative result for added reuse on that input.', '',
                  '| Window | Request | Block | 04 reuses (5a) | 06 reuses (789) | Added events | 06 fallback units |',
                  '| --- | --- | ---: | ---: | ---: | ---: | ---: |']
        for label, matches in data.get('native_count_deltas_vs04', {}).items():
            for row in matches:
                if row['request'] not in ('cold', 'broad', 'private-001'):
                    continue
                lines.append(f"| {label} | {row['request']} | {row['block']} | {row['reuse_events04']} | "
                             f"{row['reuse_events06']} | {row['added_events']} | {row['fallback_units06']} |")
    for label, w in data['windows'].items():
        lines += ['', '## ' + label, '']
        if w['state'] != 'valid':
            lines += ['Comparisons remain empty; no workflow or storage advantage is inferred.', '',
                      '`' + json.dumps(w.get('failure') or w.get('issues'), ensure_ascii=False) + '`']
            if w.get('failed_quality_records'):
                lines += ['', f"Recorded rows: {w.get('actual_rows')}/{w.get('expected_rows')}. Quality: `"
                          + json.dumps(w['failed_quality_records'], ensure_ascii=False) + '`']
            continue
        lines += ['| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |',
                  '| --- | ---: | ---: | ---: | ---: | ---: | ---: |']
        for name, g in w['contributions'].items():
            values = [g['total_ms'][a] / 1000 for a in ARMS] + [g['saved_vs_main_ms'] / 1000, g['saved_vs_rust_ms'] / 1000]
            lines.append(f"| {name} | {g['frequency']} | " + ' | '.join(f'{v:.3f}' for v in values) + ' |')
        lines += ['', 'Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. '
                  'Growth is the median of block-specific cold→unchanged-after-edits endpoints.', '',
                  '| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |',
                  '| --- | ---: | ---: | ---: | ---: |']
        for arm in ARMS:
            p = w['disk']['peaks'][arm]
            growth = w['disk']['median_growth_private'][arm]['physical_bytes']
            frequency = w['contributions']['private']['frequency']
            lines.append(f"| {arm} | {p['physical_bytes']['value'] / 2**20:.3f} | "
                         f"{p['logical_bytes']['value'] / 2**20:.3f} | {frequency} | {growth / 2**20:.3f} |")
        lines += ['', '| Request | Candidate reused functions by block | Normalized functions by block | Fallback units by block | Candidate phase ms by block |',
                  '| --- | --- | --- | --- | --- |']
        for request in w['history']:
            name = request['name']
            if name not in ('cold', 'broad', 'recovery', 'warm-restart') and not name.startswith('private-'):
                continue
            details = w['observed_request_details'][name]['candidate']['by_block']
            reused = [d['emission']['successful_reused_functions'] for d in details]
            normalized = [d['emission']['emission_sums']['normalized_functions'] for d in details]
            fallback = [d['emission']['fallback_units'] for d in details]
            phases = [{k.removeprefix('phase_timings_ns.'): round(v / 1e6, 3)
                       for k, v in d['stats'].items() if k.startswith('phase_timings_ns.')} for d in details]
            lines.append(f'| {name} | `{reused}` | `{normalized}` | `{fallback}` | `{json.dumps(phases)}` |')
        lines += ['', 'JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, '
                  'selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. '
                  '`null` means not observed. The table credits only non-fallback emission receipts; '
                  'JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.']
    lines += ['', '## Original5105 capacity', '',
              f"State: {data['capacity']['state']}; comparisons=[]; formal_ratio=null. "
              'Main ResourceLimit is a failed capacity request, never a fast timing reference. '
              'Historical Rust receipts remain explicitly historical.']
    cap = data['capacity']
    if cap.get('rows'):
        lines += ['', f"Recorded complete={cap.get('complete')}, protocol_valid={cap.get('protocol_valid')}, "
                  f"candidate_full_source_verify_native_success={cap.get('candidate_full_source_verify_native_success')}.", '',
                  '| Arm | Mode/request | Status | Current Verify runs | Wall ms (diagnostic only) | Emission reuse events | Normalized functions |',
                  '| --- | --- | --- | ---: | ---: | ---: | ---: |']
        for row in cap['rows']:
            wall = row.get('wall_ms')
            wall_text = f'{wall:.3f}' if isinstance(wall, (int, float)) else '—'
            reuse = (row.get('current_emission') or {}).get('successful_reused_functions')
            normalized = ((row.get('current_emission') or {}).get('emission_sums') or {}).get('normalized_functions')
            label = row.get('label', '') + (' [historical receipt]' if row.get('historical_only') else '')
            lines.append(f"| {row['arm']} | {row['mode']}/{label} | {row.get('status')} | "
                         f"{row.get('verification_runs')} | {wall_text} | {reuse} | {normalized} |")
        lines += ['', 'Only the recorded statuses, Verify counts, source-inventory flags, oracle observations and '
                  'emission counters support capacity claims. This is not a matched performance window. '
                  'Missing receipt counters remain unknown; no 04 capacity result is substituted.']
        counts = cap['independent_observation_counts']
        lines += ['', f"Independently counted from capacity rows: {counts['candidate_requests_with_V1_and_inventory']}/"
                  f"{counts['candidate_source_requests']} candidate Source requests have Success, Verify=1, "
                  'one current Verify event and the complete 102-file inventory. Valid native expected/actual matches: `'
                  + json.dumps(counts['native_valid_observations_by_arm']) + '`; '
                  'these process counts are separate from Verify runs and workflow timings.']
    lines += ['', '## Explicit historical check04 references', '',
              f'Both checks below were measured at `{PRIOR_SOURCE}`, not `{SOURCE}`. '
              'Production differences are native/emission.rs plus the normalized_functions observation field/JSON '
              'in LLVM API lib.rs (verified during preparation); driver/frontend/transport/Verify are unchanged. '
              'Per user authorization they remain check-path context; this is not a fresh 789 check measurement '
              'or a binary/timing equivalence claim.', '',
              '| Original check04 window | Measured source | Main s | 5a candidate s | Rust s |',
              '| --- | --- | ---: | ---: | ---: |']
    for label, old in data['prior_wave04']['reused_check04'].items():
        totals = old['totals_ms']
        lines.append(f'| {label} | 5a143457 | ' + ' | '.join(f'{totals[a]/1000:.3f}' for a in ARMS) + ' |')
    lines += ['', '## Scope and limits', '']
    lines += ['- ' + limit for limit in LIMITS]
    lines += ['', 'Prepared invocation (run only after the batch-done notice):', '',
              '`python3 -B /workspace/zil-recovery/critical-path-wave06-weighted-analysis.py --analyze`', '']
    return '\n'.join(lines)


def self_check():
    fake = [{'request': name, 'median_ms': dict(zip(ARMS, values))}
            for name, values in [('cold', (10, 12, 8)), ('broad', (20, 15, 17)),
                                 ('private-001', (3, 2, 1)), ('private-002', (5, 2, 3))]]
    groups = weighted(fake)
    require(groups['cold']['saved_vs_main_ms'] == -2, 'negative contribution lost')
    require(groups['private']['frequency'] == 2 and groups['private']['total_ms']['baseline'] == 8,
            'actual private frequency lost')
    snapshots = []
    for a, b in [(10, 0), (0, 10)]:
        categories = {f'candidate.{k}': dict.fromkeys(METRICS, v) for k, v in [('a', a), ('b', b)]}
        snapshots.append({'categories': categories,
                          'total': {m: sum(c[m] for c in categories.values()) for m in METRICS}})
    require(max(arm_total(s, 'candidate')['physical_bytes'] for s in snapshots) == 10,
            'snapshot sum before max failed')
    empty = emission({})
    require(empty['emission_sums']['reused_functions'] is None and empty['fallback_units'] is None,
            'missing evidence fabricated as zero')
    fallback = emission({'native_codegen': {'object_reports': [
        {'emission_reuse': {'reused_functions': 10, 'fallback': True}},
        {'emission_reuse': {'reused_functions': 3, 'fallback': False}}]}})
    require(fallback['emission_sums']['reused_functions'] == 13
            and fallback['successful_reused_functions'] == 3
            and fallback['fallback_units'] == 1, 'fallback work counted as delivered reuse')
    require(empty['emission_sums']['normalized_functions'] is None,
            'missing normalization observation fabricated as zero')
    observed = emission({'native_codegen': {'object_reports': [
        {'emission_reuse': {'reused_functions': 2092, 'normalized_functions': 0, 'fallback': False}}]}})
    require(observed['emission_sums']['normalized_functions'] == 0
            and observed['successful_reused_functions'] == 2092,
            'normalization count conflated with observed reuse')
    print('In-memory checks passed; no windows read, no compiler/Git/benchmark invoked.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group()
    action.add_argument('--prepare', action='store_true')
    action.add_argument('--analyze', action='store_true')
    action.add_argument('--self-check', action='store_true')
    parser.add_argument('--runs-root', type=Path, default=Path('/workspace/zil-push-main/out/runs'))
    args = parser.parse_args()
    if args.self_check:
        self_check()
        return
    windows = {label: analyze_window(args.runs_root / label) if args.analyze else
               {'state': 'awaiting-batch-done', 'comparisons': [], 'path': str(args.runs_root / label), 'issues': []}
               for label in LABELS}
    cap_root = args.runs_root / ('large-project-original5105-current-06-' + SOURCE[:12])
    prior = prior_context()
    data = {'schema': 'critical-path-wave06-weighted-coverage-v1',
            'generated_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'state': 'analyzed' if args.analyze else 'prepared-awaiting-batch-done',
            'candidate_source': SOURCE, 'main_source': MAIN, 'tool_source': TOOL,
            'current_gates': current_gates(args.runs_root, windows) if args.analyze else {'state': 'awaiting-batch-done'},
            'prior_wave04': prior,
            'native_count_deltas_vs04': counter_deltas(windows, prior) if args.analyze else {},
            'windows': windows, 'capacity': capacity(cap_root) if args.analyze else
            {'state': 'awaiting-batch-done', 'path': str(cap_root), 'comparisons': [], 'formal_ratio': None},
            'limits': LIMITS,
            'method': {'timing': 'sum(per-request median across recorded blocks)',
                       'contributions': 'actual recorded history frequency; signed reference minus candidate',
                       'disk': 'MAX(snapshot SUM(arm categories)); inherited cross-root inode deduplication',
                       'private_growth': 'per-block endpoint delta; median over blocks; include every epoch',
                       'failed_window': 'comparisons empty; no timing/disk advantages',
                       'identity_scope': 'recorded source/binary/tool/build bindings; small metadata hashes, no compiler rehash/execution'}}
    output = Path('/workspace/zil-recovery')
    (output / 'critical-path-wave06-weighted-coverage.json').write_text(json.dumps(data, indent=2, ensure_ascii=False, allow_nan=False) + '\n')
    (output / 'critical-path-wave06-weighted-coverage.md').write_text(render(data))
    print(data['state'])
    for label, result in windows.items():
        print(label, result['state'])


if __name__ == '__main__':
    main()

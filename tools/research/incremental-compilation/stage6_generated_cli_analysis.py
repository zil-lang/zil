#!/usr/bin/env python3
"""Audit a frozen 23-shape, six-block CLI window, retaining refusals and regressions."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import random
import shutil
import statistics
import types

import stage6_cli_analysis as common

HERE = Path(__file__).resolve().parent
REQUESTS = [('cold', 0), ('unchanged', 0), ('early_edit', 1), ('late_edit', 2), ('restore', 0)]
require, read, write = common.require, common.read, common.write
identity, digest, checked = common.identity, common.digest, common.checked


def expected(case, edit):
    totals = []
    for module, size in enumerate(case['sizes']):
        subtotal = 0
        for helper in range(size):
            x, constant = helper % 5, helper % 7 + 1
            if edit == 1 and module == 0 and helper == 0:
                constant += 1
            if edit == 2 and module == len(case['sizes']) - 1 and helper == size - 1:
                constant += 2
            if case['flavor'] == 'body':
                value = x + 64 + constant
            elif case['flavor'] == 'branch':
                value = (x if x < 3 else 2 * x) + constant
            elif case['flavor'] == 'ct':
                value = x + 2 * constant - 1
            else:
                require(case['flavor'] in ['scalar', 'generic', 'types'], 'Unknown source flavor')
                value = x + constant
            subtotal += value
        totals.append(subtotal)
    return sum(totals) + ((len(totals) - 1) * totals[0] if case.get('graph') == 'fanout' else 0)


def hashes(folder, extension):
    return {path.name: digest(path) for path in sorted(folder.glob('*.' + extension))}


def sources(root, output, manifest, reference):
    # SourceFileLoader can create __pycache__ beside the measured input. Compile
    # the verified bytes directly so all derived files stay under this output.
    generator = types.ModuleType('frozen_generated_cli_source')
    generator.__file__ = str(root / 'runner.py')
    exec(compile((root / 'runner.py').read_bytes(), generator.__file__, 'exec'), generator.__dict__)
    require(manifest['cases'] == generator.shapes() and len(manifest['cases']) == 23,
            'Shape metadata differs from the frozen source generator')
    reproduced = output / 'source-reproduction'
    reproduced.mkdir()
    inventory, identities = {}, {}
    for case in manifest['cases']:
        name = case['name']
        inventory[name] = {'shape': case, 'variants': {}, 'original_source_metrics': read(root / ('source-' + name) / 'metrics.json')}
        for edit in [0, 1, 2]:
            folder = reproduced / (name + '-' + str(edit))
            generated_expected = generator.generate(case, folder, edit)
            independently_expected = expected(case, edit)
            require(generated_expected == independently_expected, 'Independent source arithmetic differs')
            variants = {lang: hashes(folder, ext) for lang, ext in [('zil', 'zil'), ('rust', 'rs')]}
            inventory[name]['variants'][str(edit)] = {'files': variants, 'independent_expected': independently_expected}
            for arm in ['zil', reference]:
                language = 'rust' if arm == 'rust' else 'zil'
                for request, request_edit in REQUESTS:
                    if request_edit != edit:
                        continue
                    sidecar = root / 'source-identities' / (name + '-' + request + '-' + arm + '.json')
                    recorded = read(sidecar)
                    require(recorded == variants[language], 'Recorded edited source differs from frozen reproduction: ' + str(sidecar))
                    identities[name, request, arm] = hashlib.sha256(json.dumps(recorded, sort_keys=True).encode()).hexdigest()
            if edit == 0:
                original = root / ('source-' + name)
                for lang, ext in [('zil', 'zil'), ('rust', 'rs')]:
                    require(hashes(original, ext) == variants[lang], 'Retained restore source differs from baseline')
                    files = sorted(original.glob('*.' + ext))
                    metrics = {'files': len(files), 'bytes': sum(path.stat().st_size for path in files),
                               'max_file_bytes': max(path.stat().st_size for path in files),
                               'lines': sum(len(path.read_text().splitlines()) for path in files)}
                    require(metrics == inventory[name]['original_source_metrics'][ext], 'Retained source metrics differ')
            # Only newly generated audit files are removed, never measured input.
            shutil.rmtree(folder)
    reproduced.rmdir()
    write(output / 'reproduced-source-identities.json', inventory)
    return identities, inventory


def setup(root, output):
    manifest = read(root / 'manifest.json')
    execution = read(root.with_name(root.name + '-execution.json'))
    lease = read(root.with_name(root.name + '-exclusive-lease.json'))
    window = execution['window']
    require(window in ['default', 'closed', 'isolation', 'incremental'], 'Unsupported generated window identity')
    require(manifest['blocks'] == 6 and manifest['modes'] == ['O2'] and manifest['requests'] == [list(x) for x in REQUESTS],
            'Unexpected block/mode/request protocol')
    require(manifest['source_cache'] is False and manifest['semantic_cache'] is True, 'Unexpected cache contract')
    require(manifest['source'] == execution['source_commit_at_start'] == lease['source_commit'], 'Tool source revisions differ')
    require(execution['status'] == 'CompletedRunner' and execution['exit'] == 0 and lease['status'] == 'Released',
            'Window did not complete and release its lease')
    require(execution['planned_complete_cli_requests'] == lease['planned_cli_requests'] == 1380,
            'Unexpected planned request count')
    require(execution['cleared_inherited_environment_keys'] == common.ENVIRONMENT_KEYS, 'Inherited loader environment differs')
    at = datetime.datetime.fromisoformat
    require(at(lease['started_utc']) <= at(execution['started_utc']) < at(execution['ended_utc']) <= at(lease['finished_utc']),
            'Execution is outside its recorded exclusive lease')
    require(len(manifest['cpus']) == 2 and len(set(manifest['cpus'])) == 2, 'Unexpected CPU affinity')
    require(digest(root / 'runner.py') == manifest['tool_sha256'] == execution['runner']['sha256'], 'Frozen runner differs')
    for name in ['runner', 'coordinator', 'rustc']:
        checked(execution[name])
    for record in manifest['binaries'].values():
        checked(record)
    pair, compiler_audit = common.audit_pair(execution)
    pair_arms = {arm['name']: arm for arm in pair['arms']}
    require(manifest['binaries']['zil'] == pair_arms['candidate']['launcher'], 'Wrong measured candidate launcher')
    reference = 'baseline' if window == 'isolation' else 'rust'
    require(set(manifest['binaries']) == {'zil', reference}, 'Unexpected measurement arms')
    require(manifest['reference_semantic_cache'] == (window == 'isolation'), 'Asymmetric isolation semantic cache')
    require(manifest['closed_executable'] == (window == 'closed'), 'Unexpected export contract')
    candidate_args = ['--function-merging', 'after'] + (['--incremental-o2'] if window == 'incremental' else [])
    reference_args = ['--function-merging', 'after'] if window == 'isolation' else []
    require(manifest['zil_args'] == candidate_args and manifest['reference_args'] == reference_args,
            'Unexpected per-arm options')
    expected_reference = pair_arms['original']['launcher'] if window == 'isolation' else execution['rustc']
    require(manifest['binaries'][reference] == expected_reference, 'Wrong measured reference identity')
    launch = [execution['runner']['path'], '--zil', manifest['binaries']['zil']['path'], '--rustc', execution['rustc']['path'],
              '--out', str(root), '--mode', 'O2', '--blocks', '6', '--semantic-cache',
              '--zil-arg=--function-merging', '--zil-arg=after']
    if window == 'isolation':
        launch += ['--reference-zil', expected_reference['path'], '--reference-semantic-cache',
                   '--reference-arg=--function-merging', '--reference-arg=after']
    elif window == 'closed':
        launch += ['--closed-executable']
    elif window == 'incremental':
        launch += ['--zil-arg=--incremental-o2']
    require(execution['command'][1:] == launch, 'Coordinator command differs')
    source_ids, source_inventory = sources(root, output, manifest, reference)
    return manifest, execution, lease, reference, pair_arms, compiler_audit, source_ids, source_inventory


def command_for(row, root, manifest):
    case, arm = row['case'], row['arm']
    run = root / f'{case}-O2-{row["block"]}-{row["position"]}-{arm}'
    source = root / ('source-' + case)
    if arm == 'rust':
        command = [manifest['binaries'][arm]['path'], str(source / 'main.rs'), '--edition=2024', '-A', 'dead_code',
                   '-C', 'overflow-checks=yes', '-C', 'panic=abort', '-C', 'opt-level=2', '-C', 'codegen-units=8',
                   '-C', 'incremental=' + str(run / 'incremental'), '-o', str(run / 'program')]
        command += manifest['reference_args']
    else:
        command = [manifest['binaries'][arm]['path'], 'build', str(source / 'main.zil'), '--json', '--opt', 'O2',
                   '--codegen-units', '8', '--codegen-jobs', '2', '--object-cache', str(run / 'objects'), '-o', str(run / 'program')]
        if arm == 'zil' and manifest['closed_executable']:
            command += ['--closed-executable']
        if manifest['semantic_cache'] if arm == 'zil' else manifest['reference_semantic_cache']:
            command += ['--semantic-cache', str(run / 'semantics')]
        command += manifest['zil_args'] if arm == 'zil' else manifest['reference_args']
    return command, run


def audit_rows(root, output, manifest, lease, reference, pair_arms, source_ids):
    rows = [json.loads(line) for line in (root / 'rows.jsonl').read_text().splitlines()]
    names = [case['name'] for case in manifest['cases']]
    cases = {case['name']: case for case in manifest['cases']}
    grid = [(name, 'O2', block, position, arm, request, edit) for name in names for block in range(6)
            for position, arm in enumerate(['zil', reference] if block % 2 == 0 else [reference, 'zil'])
            for request, edit in REQUESTS]
    require(len(rows) == len(grid) == lease['observed_requests'] == 1380, 'Incomplete or excess request grid')
    history, audit, status_counts, semantic_counts = {}, [], {}, {}
    native_count = verify_count = object_hits = object_misses = 0
    for row, key in zip(rows, grid):
        actual = tuple(row[field] for field in ['case', 'mode', 'block', 'position', 'arm', 'request', 'edit'])
        require(actual == key, 'Request grid or order differs')
        case, mode, block, position, arm, request, edit = key
        command, run = command_for(row, root, manifest)
        require(row['command'] == command, 'Actual compiler options differ')
        require(row['source_identity_file'] == f'source-identities/{case}-{request}-{arm}.json'
                and row['source_identity_sha256'] == source_ids[case, request, arm], 'Row input identity differs')
        oracle = expected(cases[case], edit)
        require(row['expected'] == oracle, 'Row expected result differs from independent arithmetic')
        require(row['elapsed_ns'] > 0 and (row.get('max_rss_kib') is None or row['max_rss_kib'] > 0), 'Invalid timing/resource data')
        stdout, stderr = run / (request + '.stdout'), run / (request + '.stderr')
        require(stdout.is_file() and stderr.is_file(), 'Missing raw compiler output')
        valid = row['exit'] == 0 and not row['timeout']
        report, cache = None, None
        if arm != 'rust' and not row['timeout']:
            try:
                report = read(stdout)
            except (ValueError, OSError):
                valid = False
            if report is not None:
                for field, raw in [('status', 'status'), ('verification_runs', 'verification_runs'),
                                   ('native_codegen', 'native_codegen'), ('queries', 'query_events')]:
                    require(row.get(field) == report.get(raw), 'Retained ZIL report differs: ' + field)
                valid = valid and report['status'] == 'Success' and report['verification_runs'] == 1
                status_key = case + '/' + arm + '/' + report['status']
                status_counts[status_key] = status_counts.get(status_key, 0) + 1
                semantic_key = request + '/' + arm + '/' + str(report.get('disk_cache'))
                semantic_counts[semantic_key] = semantic_counts.get(semantic_key, 0) + 1
                if valid:
                    target = pair_arms['candidate' if arm == 'zil' else 'original']
                    cache = common.object_audit(report['native_codegen'], target['pipeline_identity_tag'])
                    object_hits += cache['hits']
                    object_misses += cache['misses']
                    verify_count += 1
        if valid:
            native = row.get('native', {})
            valid = (type(native.get('exit')) is int and native['exit'] == 0 and
                     native.get('stdout_hex') == (oracle & 0xffffffff).to_bytes(4, 'little').hex()
                     and native.get('stderr_hex') == '' and not native.get('timeout') and 'launch_error' not in native)
            native_count += 1
        sequence = (case, mode, block, arm)
        prior = history.get(sequence, True)
        require(row['execution_valid'] == bool(valid) and row['sequence_history_valid'] == prior,
                'Execution or preceding-cache-history validity differs')
        require(row['valid'] == (bool(valid) and prior), 'Comparison validity differs')
        history[sequence] = row['valid']
        audit.append({'case': case, 'mode': mode, 'block': block, 'position': position, 'arm': arm,
                      'request': request, 'edit': edit, 'expected': oracle, 'status': row.get('status'),
                      'execution_valid': bool(valid), 'sequence_history_valid': prior, 'comparison_valid': row['valid'],
                      'exit': row.get('exit'), 'timeout': row.get('timeout'), 'launch_error': row.get('launch_error'),
                      'validation_error': row.get('validation_error'),
                      'elapsed_ns': row['elapsed_ns'], 'cpu_seconds': row.get('cpu_seconds'),
                      'max_rss_kib': row.get('max_rss_kib'), 'semantic_cache': report.get('disk_cache') if report else None,
                      'actual_units': (row.get('native_codegen') or {}).get('actual_units'),
                      'object_hits': cache['hits'] if cache else None, 'object_misses': cache['misses'] if cache else None,
                      'verification_runs': row.get('verification_runs'), 'stdout': identity(stdout), 'stderr': identity(stderr)})
    invalid = sum(not row['valid'] for row in rows)
    require(invalid == lease['invalid_requests'], 'Lease invalid request count differs')
    with (output / 'request-audit.jsonl').open('w') as stream:
        for row in audit:
            stream.write(json.dumps(row) + '\n')
    return rows, {'requests': len(rows), 'valid_requests': len(rows) - invalid, 'invalid_requests': invalid,
                  'native_outcomes': native_count, 'current_successful_verify_requests': verify_count,
                  'status_counts': status_counts, 'semantic_cache_counts': semantic_counts,
                  'object_hits': object_hits, 'object_misses': object_misses,
                  'scope': 'Read-only compiler/native record audit. No compiler or native program re-executed. Input SHA sidecars were checked against reproduced bytes from the frozen runner, and retained restore bytes directly rehashed. Edited source reproduction is not an original snapshot. Native observations are embedded in rows JSON. This runner recorded no per-request ELF hashes, so this audit makes no historical artifact rehash claim. DSO identity is bound by the prior pair loader probe, frozen compiler/source/wrapper/library identities and current object reports, not a per-request LD_DEBUG trace.'}


def summarize(rows, cases, reference, count, seed):
    rng = random.Random(seed)
    draws = [{case: [rng.choice([0, 2, 4]) for _ in range(3)] + [rng.choice([1, 3, 5]) for _ in range(3)]
              for case in cases} for _ in range(count)]
    index = {(row['case'], row['request'], row['arm'], row['block']): row for row in rows}
    cells, sampled = [], []
    for case in cases:
        for request, _ in REQUESTS:
            selected = {arm: [index[case, request, arm, block] for block in range(6)] for arm in ['zil', reference]}
            valid = all(row['valid'] for values in selected.values() for row in values)
            values = {arm: [row['elapsed_ns'] for row in selected[arm]] for arm in ['zil', reference]}
            point = statistics.median(values['zil']) / statistics.median(values[reference]) if valid else None
            distribution = [statistics.median(values['zil'][b] for b in draw[case]) /
                            statistics.median(values[reference][b] for b in draw[case]) for draw in draws] if valid else None
            bounds = common.interval(distribution) if valid else [None, None]
            cell = {'case': case, 'mode': 'O2', 'request': request, 'reference': reference, 'valid': valid,
                    'zil_valid': sum(row['valid'] for row in selected['zil']),
                    'reference_valid': sum(row['valid'] for row in selected[reference]),
                    'zil_median_ms': statistics.median(values['zil']) / 1e6,
                    'reference_median_ms': statistics.median(values[reference]) / 1e6,
                    'zil_over_reference': point, 'paired_bootstrap_95_low': bounds[0], 'paired_bootstrap_95_high': bounds[1],
                    'adverse_point_estimate': point > 1 if valid else None,
                    'adverse_interval': bounds[0] > 1 if valid else None,
                    'favorable_interval': bounds[1] < 1 if valid else None}
            for field, label in [('cpu_seconds', 'cpu_seconds'), ('max_rss_kib', 'peak_rss_kib')]:
                medians = {arm: statistics.median(row[field] for row in selected[arm] if row.get(field) is not None)
                           if any(row.get(field) is not None for row in selected[arm]) else None for arm in ['zil', reference]}
                cell['zil_median_' + label], cell['reference_median_' + label] = medians['zil'], medians[reference]
                cell[label + '_ratio'] = medians['zil'] / medians[reference] if valid and medians[reference] and medians['zil'] is not None else None
            cells.append(cell)
            sampled.append(distribution)
    groups = []
    for name, requests in [('all_requests', [x[0] for x in REQUESTS])] + [(r, [r]) for r, _ in REQUESTS]:
        valid_indices = [i for i, cell in enumerate(cells) if cell['request'] in requests and cell['valid']]
        distributions = [common.geometric_mean(sampled[i][draw] for i in valid_indices) for draw in range(count)] if valid_indices else []
        groups.append({'group': name, 'cells': sum(cell['request'] in requests for cell in cells),
                       'valid_cells': len(valid_indices), 'invalid_cells': sum(cell['request'] in requests and not cell['valid'] for cell in cells),
                       'geomean_ratio': common.geometric_mean(cells[i]['zil_over_reference'] for i in valid_indices),
                       'paired_bootstrap_95': common.interval(distributions) if distributions else None,
                       'adverse_cells': sum(cells[i]['adverse_point_estimate'] for i in valid_indices),
                       'adverse_intervals': sum(cells[i]['adverse_interval'] for i in valid_indices),
                       'favorable_intervals': sum(cells[i]['favorable_interval'] for i in valid_indices),
                       'geomean_peak_rss_ratio': common.geometric_mean(cells[i]['peak_rss_kib_ratio'] for i in valid_indices
                                                                    if cells[i]['peak_rss_kib_ratio'] is not None)})
    return cells, groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bootstrap', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=6302026)
    args = parser.parse_args()
    require(args.bootstrap >= 100, 'Use at least 100 exploratory draws')
    root, output = args.window.resolve(strict=True), args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for path in [Path(__file__), Path(common.__file__)]:
        shutil.copy2(path, output / path.name)
    result = {'status': 'Auditing', 'producer': identity(Path(__file__)), 'shared_analysis': identity(Path(common.__file__)),
              'window': str(root)}
    try:
        manifest, execution, lease, reference, pair_arms, compiler_audit, source_ids, source_inventory = setup(root, output)
        rows, audit = audit_rows(root, output, manifest, lease, reference, pair_arms, source_ids)
        cells, groups = summarize(rows, [case['name'] for case in manifest['cases']], reference, args.bootstrap, args.seed)
        original = read(root / 'summary.json')
        require(original['rows'] == len(rows) and len(original['cells']) == len(cells), 'Runner summary dimensions differ')
        for independent, recorded in zip(cells, original['cells']):
            require(tuple(independent[k] for k in ['case', 'mode', 'request']) == tuple(recorded[k] for k in ['case', 'mode', 'request']), 'Summary cell order differs')
            for arm, label in [('zil', 'zil'), (reference, 'reference')]:
                require(common.math.isclose(independent[label + '_median_ms'], recorded['arms'][arm]['median_ms'], rel_tol=1e-12, abs_tol=1e-9), 'Runner median differs')
                require(6 - independent[label + '_valid'] == recorded['arms'][arm]['failures'], 'Runner failure count differs')
                value = independent[label + '_median_peak_rss_kib']
                require(value == recorded['arms'][arm]['max_rss_kib'], 'Runner median peak RSS differs')
            actual, expected_ratio = independent['zil_over_reference'], recorded['zil_over_' + reference]
            require((actual is None and expected_ratio is None) or (actual is not None and expected_ratio is not None
                    and common.math.isclose(actual, expected_ratio, rel_tol=1e-12, abs_tol=1e-15)), 'Runner ratio differs')
        fields = list(cells[0])
        common.csv_write(output / 'cells.csv', cells, fields)
        common.csv_write(output / 'all-adverse-cells.csv', [x for x in cells if x['adverse_point_estimate']], fields)
        common.csv_write(output / 'all-invalid-cells.csv', [x for x in cells if not x['valid']], fields)
        resource_fields = ['case', 'mode', 'block', 'position', 'arm', 'request', 'edit', 'status', 'valid',
                           'execution_valid', 'sequence_history_valid', 'elapsed_ns', 'cpu_seconds', 'max_rss_kib']
        common.csv_write(output / 'request-resources.csv', [{field: row.get(field) for field in resource_fields} for row in rows], resource_fields)
        result.update(status='AuditComplete', source_commit=manifest['source'], comparison=execution['window'], reference=reference,
                      inputs={name: identity(root / name) for name in ['manifest.json', 'rows.jsonl', 'summary.json', 'runner.py']},
                      execution_receipt=identity(root.with_name(root.name + '-execution.json')),
                      exclusive_lease=identity(root.with_name(root.name + '-exclusive-lease.json')),
                      compiler_audit=compiler_audit, source_inventory=source_inventory, audit=audit, cells=cells, groups=groups,
                      method={'point_estimate': 'Ratio of two six-block elapsed-time medians in each fixed case/request.',
                              'bootstrap': 'Exploratory percentile paired bootstrap; independently sample 3 forward and 3 reverse blocks; share indices across both arms and all five requests within a fixed case; cases remain fixed with independent block draws.',
                              'draws': args.bootstrap, 'seed': args.seed, 'confidence_level': 0.95,
                              'aggregation': 'Equal geometric-mean weight per valid fixed cell. Every invalid cell and all resource refusals remain visible and produce no speed ratio. The valid subset does not establish universal workload coverage.',
                              'limits': 'Only this fixed 23-shape corpus and six blocks on this machine. Cache-cold is not OS-page-cache-cold; CLI startup, identity/cache work, analysis, Verify, backend and linking are timed. Native execution and source generation are outside each timer. wait4 peak RSS is not aggregate system peak memory or Session current VmRSS. Intervals describe within-window variation, not industrial workload-population uncertainty.',
                              'contracts': 'Explicit O2, after-MergeFunctions and trusted semantic-cache; source-cache disabled. Export and incremental flags are recorded separately per window. Rust uses overflow checks, panic abort, 8 CGUs and persistent per-sequence incremental cache. No compiler acceptance limit is raised for failed cells.'})
        lines = ['# Independent generated-source CLI audit', '',
                 f'Window `{execution["window"]}`: {audit["requests"]} requests, {audit["valid_requests"]} valid, '
                 f'{audit["invalid_requests"]} invalid. All 115 comparison cells remain in the tables.', '',
                 '| Request group | ZIL/reference GM | Exploratory paired 95% interval | Adverse / valid cells | Invalid cells |',
                 '|---|---:|---:|---:|---:|']
        for group in groups:
            low, high = group['paired_bootstrap_95'] or [float('nan'), float('nan')]
            ratio = group['geomean_ratio'] if group['geomean_ratio'] is not None else float('nan')
            lines.append(f'| {group["group"]} | {ratio:.6f} | [{low:.6f}, {high:.6f}] | {group["adverse_cells"]}/{group["valid_cells"]} | {group["invalid_cells"]} |')
        lines += ['', f'Reference arm: `{reference}`. A ratio above 1 is slower for the candidate. The aggregate is conditional on valid cells; refused shapes have no comparable compilation time and must not disappear from the report.', '',
                  audit['scope'], '', result['method']['limits'], '', result['method']['contracts'], '']
        (output / 'REPORT.md').write_text('\n'.join(lines))
    except BaseException as error:
        result.update(status='AuditFailed', error=repr(error))
        raise
    finally:
        write(output / 'summary.json', result)
        print(json.dumps({'status': result['status'], 'summary': str(output / 'summary.json')}))


if __name__ == '__main__':
    main()

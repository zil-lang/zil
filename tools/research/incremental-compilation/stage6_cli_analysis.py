#!/usr/bin/env python3
"""Independently audit and summarize a frozen six-block real-source CLI window.

This reader does not execute a compiler or a produced program. It checks the
retained records, source copies, final artifacts, and exact compiler identities.
"""
import argparse
import csv
import datetime
import hashlib
import io
import json
import math
from pathlib import Path
import random
import re
import shutil
import statistics
import subprocess
import tarfile


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
CASES = ['app', 'stream', 'slices', 'byte_io', 'byte_buffer', 'byte_buffer_owned',
         'resources', 'resource_closures', 'error_cleanup', 'owned_component',
         'package_ui', 'retained_ui', 'update_ui', 'helper_ui']
INPUTS = ['', '21', '44', '53', '55', '4f', '4d', '45', '54', '4c',
          '30', '31', '32', '33', '313261732b212d710080']
REQUESTS = ['cold', 'unchanged']
ENVIRONMENT_KEYS = ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT', 'LD_DEBUG',
                    'LD_DEBUG_OUTPUT', 'ZIL_CLANG', 'ZIL_LLVM_LIB_DIR']


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'sha256': digest(path), 'bytes': path.stat().st_size}


def checked(record):
    actual = identity(record['path'])
    require(actual['sha256'] == record['sha256'], 'Changed input: ' + str(record['path']))
    if 'bytes' in record:
        require(actual['bytes'] == record['bytes'], 'Changed input size: ' + str(record['path']))
    return actual


def geometric_mean(values):
    values = list(values)
    return math.exp(statistics.mean(math.log(x) for x in values)) if values else None


def percentile(values, fraction):
    ordered = sorted(values)
    at = (len(ordered) - 1) * fraction
    low, high = math.floor(at), math.ceil(at)
    return ordered[low] if low == high else ordered[low] * (high - at) + ordered[high] * (at - low)


def interval(values):
    return [percentile(values, 0.025), percentile(values, 0.975)]


def csv_write(path, rows, fields):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def native_valid(outcomes):
    if (not isinstance(outcomes, list) or len(outcomes) != len(INPUTS)
            or any(not isinstance(row, dict) for row in outcomes)
            or [row.get('stdin_hex') for row in outcomes] != INPUTS):
        return False
    for row in outcomes:
        if type(row.get('exit')) is not int or row['exit'] < 0 or 'error' in row:
            return False
        for field in ['stdout_hex', 'stderr_hex']:
            value = row.get(field)
            if not isinstance(value, str):
                return False
            try:
                if bytes.fromhex(value).hex() != value:
                    return False
            except ValueError:
                return False
    return True


def git_archive_hashes(revision, paths):
    require(isinstance(revision, str) and re.fullmatch(r'[0-9a-f]{40}', revision),
            'A complete Git source revision is required')
    archive = subprocess.check_output(['git', '-C', str(REPO), 'archive', revision, '--', *paths])
    result = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:') as stream:
        for member in stream:
            if member.isfile():
                result[member.name] = hashlib.sha256(stream.extractfile(member).read()).hexdigest()
    return result


def audit_pair(execution):
    checked(execution['pair'])
    pair = read(execution['pair']['path'])
    require(pair == execution['pair_contents'] and pair['status'] == 'Pass', 'Pair receipt differs')
    require([row['name'] for row in pair['arms']] == ['original', 'candidate'], 'Unexpected pair arms')
    require(len({row['production_revision'] for row in pair['arms']}) == 1, 'Production revisions differ')
    require(len({row['original_files_manifest_sha256'] for row in pair['arms']}) == 1,
            'Original source inventories differ')
    require(len({row['llvm']['library']['sha256'] for row in pair['arms']}) == 2, 'Library arms are not distinct')
    checked(pair['clang'])
    verified, original_git_trees = [], {}
    for arm in pair['arms']:
        for key in ['compiler', 'launcher', 'source_receipt']:
            checked(arm[key])
        llvm = {key: checked(record) for key, record in arm['llvm'].items()}
        source = read(arm['source_receipt']['path'])
        native = read(arm['llvm']['receipt']['path'])
        require(checked(native['library']) == llvm['library'], 'Native receipt identifies a different library')
        require(checked(source['toolchain_receipt']) == llvm['receipt'], 'Source identifies a different toolchain receipt')
        require(native['status'] == 'Pass' and native['arm'] == source['arm'] == arm['toolchain_arm'],
                'Toolchain/source arm mismatch')
        require(source['revision'] == arm['production_revision'] and source['production_mutated'] is False,
                'Wrong compiler source revision or production mutation')
        require(source['pipeline_identity_tag'] == arm['pipeline_identity_tag'], 'Pipeline tag differs')
        require(source['original_files_manifest_sha256'] == arm['original_files_manifest_sha256'],
                'Source inventory metadata differs')
        snapshot = Path(arm['source_receipt']['path']).parent
        originals_path = snapshot / 'SOURCE-ORIGINALS.json'
        require(digest(originals_path) == source['original_files_manifest_sha256'], 'Source inventory changed')
        originals = read(originals_path)
        tree_key = (source['revision'], source['original_files_manifest_sha256'])
        if tree_key not in original_git_trees:
            original_git_trees[tree_key] = git_archive_hashes(source['revision'], [record['path'] for record in originals])
        require(original_git_trees[tree_key] == {record['path']: record['sha256'] for record in originals},
                'Original source inventory differs from the claimed production Git revision')
        for record in originals:
            expected = source['patched_sha256'] if record['path'] == source['modified_file'] else record['sha256']
            require(digest(snapshot / record['path']) == expected, 'Snapshot input changed: ' + record['path'])
        require(digest(snapshot / 'RESEARCH.patch') == source['patch_sha256'], 'Compiler identity patch changed')
        tag = '|research-llvm-stage6=' + arm['toolchain_arm'] + '|dso-sha256=' + arm['llvm']['library']['sha256']
        require(tag == arm['pipeline_identity_tag'], 'Unexpected pipeline tag encoding')
        require(tag.encode() in Path(arm['compiler']['path']).read_bytes(), 'ELF lacks frozen pipeline tag')
        checked(arm['loader_probe']['log'])
        require(checked(arm['loader_probe']['actual_library']) == llvm['library'],
                'Loader probe identifies a different library')
        loader = Path(arm['loader_probe']['log']['path']).read_text()
        loaded = {str(Path(name).resolve()) for name in re.findall(r'calling init:\s*(/\S*libLLVM\S*)', loader)}
        require(loaded == {str(Path(arm['llvm']['library']['path']).resolve())}, 'Pair loader probe differs')
        require(arm['loader_probe']['returncode'] == 0, 'Pair loader probe did not pass')
        verified.append({'arm': arm['name'], 'production_revision': arm['production_revision'],
                         'compiler': arm['compiler'], 'library': arm['llvm']['library'],
                         'launcher': arm['launcher'], 'pipeline_identity_tag': tag,
                         'checked_snapshot_files': len(originals), 'source_receipt': arm['source_receipt'],
                         'loader_probe': arm['loader_probe']})
    return pair, verified


def source_audit(root, manifest):
    git_files = git_archive_hashes(manifest['source'], ['examples/mainline'])
    records = {}
    for case in CASES:
        folder = root / ('source-' + case)
        expected = read(root / (case + '-sources.json'))
        actual = {str(path.relative_to(folder)): digest(path) for path in sorted(folder.rglob('*')) if path.is_file()}
        require(actual == expected, 'Copied source tree differs: ' + case)
        git_expected = {name.removeprefix('examples/mainline/' + case + '/'): value
                        for name, value in git_files.items() if name.startswith('examples/mainline/' + case + '/')}
        require(actual == git_expected, 'Copied source tree differs from frozen Git source: ' + case)
        combined = hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()
        records[case] = {'source_identity_sha256': combined, 'files': expected,
                         'bytes': sum(path.stat().st_size for path in folder.rglob('*') if path.is_file()),
                         'file_count': len(expected)}
    return records


def expected_command(row, root, manifest):
    case, mode, arm = row['case'], row['mode'], row['arm']
    run = root / f'{case}-{mode}-{row["block"]}-{row["position"]}-{arm}'
    source = root / ('source-' + case)
    if arm == 'zil':
        command = [manifest['binaries']['zil']['path'], 'build', str(source / 'main.zil'), '--json',
                   '--opt', 'O2', '--codegen-units', '8', '--codegen-jobs', '2',
                   '--object-cache', str(run / 'objects'), '-o', str(run / 'program')]
        if manifest['closed_executable']:
            command += ['--closed-executable']
        if manifest['semantic_cache']:
            command += ['--semantic-cache', str(run / 'semantics')]
        command += manifest['zil_args']
    else:
        command = [manifest['binaries']['rust']['path'], str(source / 'main.rs'), '--edition=2024',
                   '-A', 'warnings', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                   '-C', 'opt-level=2', '-C', 'codegen-units=8', '-C', 'incremental=' + str(run / 'incremental'),
                   '-o', str(run / 'program')]
    return command, run


def object_audit(native, tag):
    require(native['backend'] == 'llvm-api' and native['actual_units'] > 0, 'Wrong actual backend')
    require(native['requested_units'] == 8 and 0 < native['jobs'] <= 2, 'Unexpected CGU/job request')
    require(len(native['object_reports']) == native['actual_units'], 'Incomplete object reports')
    for item in native['object_reports']:
        require(item['pipeline'] == 'default<O2>|merge-functions=after|aliases=false' + tag,
                'Different actual optimization pipeline')
        require(item['backend'] == 'llvm-api' and item['llvm_version'] == '23.1.4', 'Wrong object producer')
        if item['input_kind'] == 'object-cache':
            require(item['cache']['state'] == 'hit' and item['producer_verified_before_after'] is True
                    and item['llvm_verification_this_request'] is False, 'Wrong cached verification boundary')
        else:
            require(item['input_kind'] == 'builder' and item['verify_before_after'] is True,
                    'New object lacks LLVM pre/post verification')
    cache = native['object_cache']
    require(cache['requested'] and cache['enabled'] and cache['disabled_reason'] is None,
            'Object cache unexpectedly disabled')
    require(cache['hits'] + cache['misses'] == native['actual_units'], 'Object cache accounting differs')
    require(cache['hits'] == sum(item['cache']['state'] == 'hit' for item in native['object_reports'])
            and cache['misses'] == sum(item['cache']['state'] == 'miss' for item in native['object_reports']),
            'Aggregate cache counts differ from actual object states')
    return cache


def audit_real(root, output):
    manifest = read(root / 'manifest.json')
    execution_path = root.with_name(root.name + '-execution.json')
    lease_path = root.with_name(root.name + '-exclusive-lease.json')
    execution, lease = read(execution_path), read(lease_path)
    require(execution['window'] == 'real', 'This analyzer currently audits the real-source protocol only')
    require(manifest['cases'] == CASES and manifest['blocks'] == 6 and manifest['modes'] == ['O2'],
            'Unexpected real-source protocol dimensions')
    require(manifest['stdin_hex'] == INPUTS and manifest['zil_args'] == ['--function-merging', 'after'],
            'Unexpected native inputs or merge policy')
    require(manifest['semantic_cache'] is True and manifest['closed_executable'] is False,
            'Unexpected cache or export contract')
    require(manifest['cpus'] == [0, 1] and len(set(manifest['cpus'])) == 2, 'Unexpected CPU affinity')
    require(execution['source_commit_at_start'] == lease['source_commit'] == manifest['source'],
            'Tool checkout identities differ')
    require(execution['planned_complete_cli_requests'] == lease['planned_cli_requests'] == 336,
            'Unexpected planned request count')
    require(execution['status'] == 'CompletedRunner' and execution['exit'] == 0 and lease['status'] == 'Released',
            'Window did not complete and release its lease')
    require(execution['cleared_inherited_environment_keys'] == ENVIRONMENT_KEYS, 'Inherited loader environment differs')
    require(execution['command'][1:] == [execution['runner']['path'], '--zil', manifest['binaries']['zil']['path'],
            '--rustc', manifest['binaries']['rust']['path'], '--out', str(root), '--mode', 'O2', '--blocks', '6',
            '--semantic-cache', '--zil-arg=--function-merging', '--zil-arg=after'],
            'Coordinator launch options differ')
    at = lambda value: datetime.datetime.fromisoformat(value)
    require(at(lease['started_utc']) <= at(execution['started_utc']) < at(execution['ended_utc']) <= at(lease['finished_utc']),
            'Execution is outside the recorded exclusive lease')
    require(digest(root / 'runner.py') == manifest['tool_sha256'] == execution['runner']['sha256'],
            'Frozen real runner differs')
    require(digest(root / 'distribution_bench.py') == manifest['runner_sha256'], 'Frozen source utility differs')
    require(digest(root / 'stage6_bench.py') == manifest['timing_runner_sha256'], 'Frozen timer differs')
    for name in ['runner', 'coordinator', 'rustc']:
        checked(execution[name])
    for record in manifest['binaries'].values():
        checked(record)
    require(manifest['binaries']['rust'] == execution['rustc'], 'Rust compiler identities differ')
    pair, pair_verified = audit_pair(execution)
    candidate = pair['arms'][1]
    require(manifest['binaries']['zil'] == candidate['launcher'], 'Measured wrapper is not candidate launcher')
    sources = source_audit(root, manifest)
    oracles = {}
    for case in CASES:
        folder = root / ('oracle-' + case)
        record = read(folder / 'record.json')
        expected_command_line = [manifest['binaries']['rust']['path'], str(root / ('source-' + case) / 'main.rs'),
                                 '--edition=2024', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                                 '-C', 'opt-level=0', '-o', str(folder / 'program')]
        require(record['command'] == expected_command_line, 'Oracle compile command differs')
        require(record['native_arguments'] == (['--check'] if case == 'app' else []), 'Oracle arguments differ')
        independently_valid = record['exit'] == 0 and not record['timeout'] and native_valid(record['native'])
        require(record['oracle_valid'] == independently_valid, 'Oracle validity differs')
        oracles[case] = record
    rows = [json.loads(line) for line in (root / 'rows.jsonl').read_text().splitlines()]
    grid = [(case, 'O2', block, position, arm, request) for case in CASES for block in range(6)
            for position, arm in enumerate(['zil', 'rust'] if block % 2 == 0 else ['rust', 'zil'])
            for request in REQUESTS]
    require(len(rows) == len(grid) == lease['observed_requests'] == 336, 'Incomplete or excess request grid')
    audit, final_artifacts, history = [], [], {}
    for row, expected_grid in zip(rows, grid):
        actual_grid = tuple(row[key] for key in ['case', 'mode', 'block', 'position', 'arm', 'request'])
        require(actual_grid == expected_grid, 'Request order/identity differs: ' + str(actual_grid))
        case, mode, block, position, arm, request = actual_grid
        command, run = expected_command(row, root, manifest)
        require(row['command'] == command, 'Compiler command differs: ' + str(actual_grid))
        require(row['source_identity_sha256'] == sources[case]['source_identity_sha256'], 'Row source identity differs')
        require(row['elapsed_ns'] > 0 and (row.get('max_rss_kib') is None or row['max_rss_kib'] > 0),
                'Invalid raw timing/resource number')
        stdout, stderr = run / (request + '.stdout'), run / (request + '.stderr')
        require(stdout.is_file() and stderr.is_file(), 'Missing raw compiler output')
        valid = row['exit'] == 0 and not row['timeout'] and oracles[case]['oracle_valid']
        report, cache = None, None
        if arm == 'zil':
            try:
                report = read(stdout)
            except (ValueError, OSError):
                valid = False
            if report is not None:
                for field, report_field in [('status', 'status'), ('verification_runs', 'verification_runs'),
                                             ('native_codegen', 'native_codegen'), ('disk_cache', 'disk_cache'),
                                             ('queries', 'query_events')]:
                    require(row.get(field) == report.get(report_field), 'Raw ZIL report differs: ' + field)
                valid = valid and report['status'] == 'Success' and report['verification_runs'] == 1
                if valid:
                    cache = object_audit(report['native_codegen'], candidate['pipeline_identity_tag'])
        if valid:
            require(row.get('native_arguments') == (['--check'] if case == 'app' and arm == 'rust' else []),
                    'Native invocation arguments differ')
            valid = native_valid(row.get('native')) and row['native'] == oracles[case]['native']
        key = (case, block, arm)
        prior = history.get(key, True)
        require(row['execution_valid'] == bool(valid) and row['sequence_history_valid'] == prior,
                'Execution validity or cache history differs')
        require(row['valid'] == (bool(valid) and prior), 'Comparison validity differs')
        history[key] = row['valid']
        if request == 'unchanged' and row['execution_valid']:
            program = run / 'program'
            require(program.stat().st_size == row['exe_bytes'] and digest(program) == row['exe_sha256'],
                    'Retained final artifact differs')
            final_artifacts.append({'case': case, 'block': block, 'arm': arm, 'identity': identity(program),
                                    'scope': 'Only this final unchanged artifact is retained; cold artifacts may have been overwritten.'})
        audit.append({'case': case, 'mode': mode, 'block': block, 'position': position, 'arm': arm,
                      'request': request, 'execution_valid': bool(valid), 'sequence_history_valid': prior,
                      'comparison_valid': row['valid'], 'status': row.get('status'),
                      'elapsed_ns': row['elapsed_ns'], 'cpu_seconds': row.get('cpu_seconds'),
                      'max_rss_kib': row.get('max_rss_kib'), 'semantic_cache': row.get('disk_cache'),
                      'actual_units': row.get('native_codegen', {}).get('actual_units'),
                      'object_hits': cache['hits'] if cache else None, 'object_misses': cache['misses'] if cache else None,
                      'verification_runs': row.get('verification_runs'), 'native_outcomes': len(row.get('native', [])),
                      'stdout': identity(stdout), 'stderr': identity(stderr)})
    require(sum(not row['valid'] for row in rows) == lease['invalid_requests'], 'Lease invalid count differs')
    with (output / 'request-audit.jsonl').open('w') as stream:
        for row in audit:
            stream.write(json.dumps(row) + '\n')
    return manifest, execution, lease, rows, {'pair': pair_verified, 'sources': sources,
        'retained_final_artifacts': final_artifacts, 'oracle_native_outcomes': sum(len(x['native'] or []) for x in oracles.values()),
        'requests': len(rows), 'invalid_requests': sum(not x['valid'] for x in rows),
        'native_outcomes': sum(len(x.get('native', [])) for x in rows),
        'current_successful_verify_requests': sum(x['arm'] == 'zil' and x['execution_valid'] and x.get('verification_runs') == 1 for x in rows),
        'scope': 'Read-only audit; no compiler or native program re-executed. Real native outputs are retained in row/oracle JSON, not separate raw process files. Final unchanged ELF per sequence is rehashed; earlier cold ELF hashes remain historical row records. DSO binding uses frozen pair loader probes, checked ELF/DSO/wrapper/source identities and per-request object pipeline reports; there is no per-request LD_DEBUG trace.'}


def summarize(rows, draws, seed):
    rng = random.Random(seed)
    samples = [{case: [rng.choice([0, 2, 4]) for _ in range(3)] + [rng.choice([1, 3, 5]) for _ in range(3)]
                for case in CASES} for _ in range(draws)]
    index = {(row['case'], row['request'], row['arm'], row['block']): row for row in rows}
    cells, sampled = [], []
    for case in CASES:
        for request in REQUESTS:
            selected = {arm: [index[case, request, arm, block] for block in range(6)] for arm in ['zil', 'rust']}
            valid = all(row['valid'] for value in selected.values() for row in value)
            top, bottom = [[row['elapsed_ns'] for row in selected[arm]] for arm in ['zil', 'rust']]
            point = statistics.median(top) / statistics.median(bottom) if valid else None
            distribution = [statistics.median(top[b] for b in draw[case]) / statistics.median(bottom[b] for b in draw[case])
                            for draw in samples] if valid else None
            bounds = interval(distribution) if valid else [None, None]
            cell = {'case': case, 'mode': 'O2', 'request': request, 'valid': valid,
                    'zil_valid': sum(row['valid'] for row in selected['zil']),
                    'rust_valid': sum(row['valid'] for row in selected['rust']),
                    'zil_median_ms': statistics.median(top) / 1e6, 'rust_median_ms': statistics.median(bottom) / 1e6,
                    'zil_over_rust': point, 'paired_bootstrap_95_low': bounds[0], 'paired_bootstrap_95_high': bounds[1],
                    'adverse_point_estimate': point > 1 if valid else None,
                    'adverse_interval': bounds[0] > 1 if valid else None,
                    'favorable_interval': bounds[1] < 1 if valid else None}
            for field, label in [('cpu_seconds', 'cpu_seconds'), ('max_rss_kib', 'peak_rss_kib')]:
                medians = {arm: statistics.median(row[field] for row in selected[arm] if row.get(field) is not None)
                           if any(row.get(field) is not None for row in selected[arm]) else None for arm in ['zil', 'rust']}
                cell.update({arm + '_median_' + label: value for arm, value in medians.items()})
                cell[label + '_ratio'] = medians['zil'] / medians['rust'] if valid and medians['rust'] and medians['zil'] is not None else None
            cells.append(cell)
            sampled.append(distribution)
    groups = []
    for name, requests in [('all_requests', REQUESTS), ('cold', ['cold']), ('unchanged', ['unchanged'])]:
        indices = [i for i, cell in enumerate(cells) if cell['request'] in requests and cell['valid']]
        ratios = [cells[i]['zil_over_rust'] for i in indices]
        boot = [geometric_mean(sampled[i][draw] for i in indices) for draw in range(draws)] if indices else []
        groups.append({'group': name, 'cells': sum(cell['request'] in requests for cell in cells),
                       'valid_cells': len(indices), 'invalid_cells': sum(cell['request'] in requests and not cell['valid'] for cell in cells),
                       'geomean_ratio': geometric_mean(ratios), 'paired_bootstrap_95': interval(boot) if boot else None,
                       'adverse_cells': sum(cells[i]['adverse_point_estimate'] for i in indices),
                       'adverse_intervals': sum(cells[i]['adverse_interval'] for i in indices),
                       'favorable_intervals': sum(cells[i]['favorable_interval'] for i in indices),
                       'geomean_peak_rss_ratio': geometric_mean(cells[i]['peak_rss_kib_ratio'] for i in indices
                                                              if cells[i]['peak_rss_kib_ratio'] is not None)})
    return cells, groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bootstrap', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=6302026)
    args = parser.parse_args()
    require(args.bootstrap >= 100, 'Use at least 100 exploratory bootstrap draws')
    root, output = args.window.resolve(strict=True), args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy2(Path(__file__), output / Path(__file__).name)
    result = {'status': 'Auditing', 'producer': identity(Path(__file__)), 'window': str(root)}
    try:
        manifest, execution, lease, rows, audit = audit_real(root, output)
        cells, groups = summarize(rows, args.bootstrap, args.seed)
        reported = read(root / 'summary.json')
        require(reported['rows'] == len(rows) and len(reported['cells']) == len(cells), 'Runner summary dimensions differ')
        for independent, original in zip(cells, reported['cells']):
            require(tuple(independent[k] for k in ['case', 'mode', 'request']) == tuple(original[k] for k in ['case', 'mode', 'request']),
                    'Runner summary cell order differs')
            for arm in ['zil', 'rust']:
                require(math.isclose(independent[arm + '_median_ms'], original['arms'][arm]['median_ms'],
                                     rel_tol=1e-12, abs_tol=1e-9), 'Runner median differs')
                require(6 - independent[arm + '_valid'] == original['arms'][arm]['failures'], 'Runner failure count differs')
            require((independent['zil_over_rust'] is None and original['zil_over_rust'] is None) or
                    (independent['zil_over_rust'] is not None and original['zil_over_rust'] is not None and
                     math.isclose(independent['zil_over_rust'], original['zil_over_rust'], rel_tol=1e-12, abs_tol=1e-15)),
                    'Runner ratio differs')
        fields = list(cells[0])
        csv_write(output / 'cells.csv', cells, fields)
        csv_write(output / 'all-adverse-cells.csv', [cell for cell in cells if cell['adverse_point_estimate']], fields)
        csv_write(output / 'all-invalid-cells.csv', [cell for cell in cells if not cell['valid']], fields)
        csv_write(output / 'request-resources.csv', [{key: row.get(key) for key in
                  ['case', 'mode', 'block', 'position', 'arm', 'request', 'valid', 'execution_valid', 'sequence_history_valid',
                   'elapsed_ns', 'cpu_seconds', 'max_rss_kib', 'exe_bytes', 'exe_sha256']} for row in rows],
                  ['case', 'mode', 'block', 'position', 'arm', 'request', 'valid', 'execution_valid', 'sequence_history_valid',
                   'elapsed_ns', 'cpu_seconds', 'max_rss_kib', 'exe_bytes', 'exe_sha256'])
        result.update(status='AuditComplete', inputs={name: identity(root / name) for name in
                      ['manifest.json', 'rows.jsonl', 'summary.json', 'runner.py', 'distribution_bench.py', 'stage6_bench.py']},
                      execution_receipt=identity(root.with_name(root.name + '-execution.json')),
                      exclusive_lease=identity(root.with_name(root.name + '-exclusive-lease.json')),
                      source_commit=manifest['source'], audit=audit, cells=cells, groups=groups,
                      method={'point_estimate': 'Ratio of the two six-block elapsed-time medians for each fixed case/request.',
                              'bootstrap': 'Exploratory percentile paired bootstrap: resample 3 forward and 3 reverse order blocks separately. Within each fixed case, share the same block indices across both arms and both requests. Different cases have independent draws; cases are not resampled.',
                              'draws': args.bootstrap, 'seed': args.seed, 'confidence_level': 0.95,
                              'aggregation': 'Equal geometric-mean weight for each valid fixed comparison cell. Invalid cells remain in the full tables and never produce speed ratios.',
                              'limits': 'Six blocks on this machine and 14 small repository programs, not a random sample of industrial projects. Cold means empty compiler caches, not an empty OS page cache. Process elapsed time includes startup, compiler/cache work and linking; native oracles are outside its timer. Peak RSS is the wait4 process statistic, not Session current VmRSS. Intervals describe within-window variation only.',
                              'contracts': 'Explicit O2, after-MergeFunctions, trusted semantic candidate cache, default ZIL exports; Rust O2/overflow checks/panic abort/8 CGUs/incremental. Independent checked-in Rust pairs establish native behavior, not identical internal compiler work. Nonnegative nonzero native exits can be valid; app Rust uses --check.'})
        lines = ['# Independent real-source CLI audit', '',
                 f'{len(rows)} complete CLI requests, {audit["native_outcomes"]} retained native output observations, '
                 f'and {audit["current_successful_verify_requests"]} successful current ZIL Verify requests checked. '
                 'No compiler or produced program was re-executed.', '',
                 '| Request group | ZIL/Rust GM | Exploratory paired 95% interval | Adverse / valid cells |',
                 '|---|---:|---:|---:|']
        for group in groups:
            lo, hi = group['paired_bootstrap_95'] or [float('nan'), float('nan')]
            ratio = group['geomean_ratio'] if group['geomean_ratio'] is not None else float('nan')
            lines.append(f'| {group["group"]} | {ratio:.6f} | [{lo:.6f}, {hi:.6f}] | {group["adverse_cells"]}/{group["valid_cells"]} |')
        lines += ['', 'A ratio below 1 is faster for ZIL. Every comparison cell, including any invalid or adverse cell, is retained in the CSV tables. This is the explicit after/semantic-cache candidate, not an unparameterized compiler default.', '',
                  'Source copies match the frozen Git input tree; the actual common ZIL production revision, private compiler identity patches, ELF/DSO/wrapper bytes and prior pair loader probes are independently checked. The current repository HEAD is not substituted for the measured compiler identity.', '',
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

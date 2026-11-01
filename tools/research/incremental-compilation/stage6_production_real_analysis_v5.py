#!/usr/bin/env python3
"""Audit final-production real336 identity and reuse the frozen real-source statistics."""
import argparse
import datetime
import json
import math
from pathlib import Path
import shutil
import sys

sys.dont_write_bytecode = True
import stage6_cli_analysis as real
from stage6_production_identity_v5 import PIPELINE, load_production_manifest, load_rust_reference, validate_binding

HERE = Path(__file__).resolve().parent
require, read, write = real.require, real.read, real.write
checked, identity, digest = real.checked, real.identity, real.digest
FLAGS = ['--function-merging', 'after', '--backend', 'llvm-api', '--incremental-o2']
CORPUS_REVISION = 'df04d4920b9d3ec618e660f8149063eb6019b317'
TOOLS = ['stage6_production_real_cli_v5.py', 'stage6_production_identity_v5.py',
         'stage6_production_ledger.py', 'stage6_production_process.py', 'stage6_cli_analysis.py',
         'stage6_toolchain_runtime.py', 'stage6_merge_runtime.py', 'stage6_bench.py',
         'stage6_real_pair_bench.py', 'distribution_bench.py']


def original_reference(execution, rust):
    checked(execution['real_reference'])
    reference = read(execution['real_reference']['path'])
    root = Path(reference['root']).resolve(strict=True)
    checked(reference['manifest'])
    require(reference['manifest'] == execution['real_reference_manifest']
            and Path(reference['manifest']['path']) == root / 'manifest.json'
            and reference['corpus_revision'] == CORPUS_REVISION
            and reference['checkout_revision'] == execution['source_commit_at_start'], 'Reference identity differs')
    manifest = read(root / 'manifest.json')
    require(manifest['source'] == CORPUS_REVISION and manifest['cases'] == real.CASES
            and manifest['stdin_hex'] == real.INPUTS and manifest['modes'] == ['O2'] and manifest['blocks'] == 6
            and manifest['closed_executable'] is False and manifest['semantic_cache'] is True
            and manifest['zil_args'] == ['--function-merging', 'after'], 'Wrong historical corpus contract')
    require(manifest['binaries']['rust']['path'] == rust['compiler']['path']
            and manifest['binaries']['rust']['sha256'] == rust['compiler']['sha256'], 'Historical Rust identity differs')
    sources = real.source_audit(root, manifest)
    require(set(reference['cases']) == set(real.CASES), 'Incomplete reference cases')
    for case in real.CASES:
        item = reference['cases'][case]
        require(item['sources'] == sources[case], 'Reference source snapshot differs: ' + case)
        require(checked(item['source_manifest']) == identity(root / (case + '-sources.json')),
                'Reference source manifest differs')
        require(checked(item['oracle_record']) == identity(root / ('oracle-' + case) / 'record.json'),
                'Original oracle bytes changed')
        oracle = read(item['oracle_record']['path'])
        command = [rust['compiler']['path'], str(root / ('source-' + case) / 'main.rs'),
                   '--edition=2024', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                   '-C', 'opt-level=0', '-o', str(root / ('oracle-' + case) / 'program')]
        require(oracle['command'] == command and oracle['exit'] == 0 and oracle['timeout'] is False
                and oracle.get('launch_error') is None and oracle['oracle_valid'] is True
                and oracle['native_arguments'] == (['--check'] if case == 'app' else [])
                and real.native_valid(oracle['native']) and oracle['native'] == item['native'],
                'Invalid or changed original native outcomes')
    return reference


def setup(root):
    execution = read(root.with_name(root.name + '-execution.json'))
    lease = read(root.with_name(root.name + '-exclusive-lease.json'))
    manifest = read(root / 'manifest.json')
    require(execution['kind'] == 'production-quality-real-cli-v5' and execution['window'] == 'real'
            and execution['status'] == 'CompletedRunner' and execution['exit'] == 0
            and lease['status'] == 'Released', 'Real production window did not complete')
    require(execution['planned_complete_cli_requests'] == lease['planned_cli_requests'] == lease['observed_requests'] == 336,
            'Wrong complete CLI request count')
    require(lease['window'] == 'real' and lease['out'] == execution['out'] == str(root)
            and lease['source_commit'] == execution['source_commit_at_start'], 'Wrong real lease/output/checkout')
    for key in ['production_manifest', 'rust_reference', 'real_reference_manifest']:
        require(lease[key] == execution[key], 'Leased identity differs: ' + key)
        checked(execution[key])
    checked(execution['exclusive_lease_at_start'])
    active = read(execution['exclusive_lease_at_start']['path'])
    require(active['status'] == 'Active' and all(active[key] == lease[key] for key in
            ['window', 'out', 'source_commit', 'production_revision', 'planned_cli_requests', 'started_utc',
             'production_manifest', 'rust_reference', 'real_reference_manifest']), 'Frozen Active lease differs')
    at = datetime.datetime.fromisoformat
    require(at(lease['started_utc']) <= at(execution['started_utc']) < at(execution['ended_utc']) <= at(lease['finished_utc']),
            'Execution is outside the exclusive lease')
    require(execution['observed_requests'] == execution['completed_requests'] == execution['attempted_requests'] == 336
            and execution['not_run_requests'] == execution['attempted_incomplete_requests'] == execution['interrupted_requests']
            == execution['duplicate_request_keys'] == execution['unexpected_request_keys'] == 0
            and execution['raw_row_parse_errors'] == [], 'Incomplete or invalid request ledger')
    for key in ['not_run', 'attempted_incomplete']:
        checked(execution[key])
        require(read(execution[key]['path']) == [], 'Ledger contains unfinished requests')
    cleanup = execution['post_runner_cleanup']
    require(execution['owned_child_subreaper'] is True and cleanup['captured'] == []
            and cleanup['all_owned_processes_reaped'] is True and cleanup['no_running_descendants'] is True,
            'Owned processes remained after runner completion')
    profile = load_production_manifest(execution['production_manifest']['path'])
    require(profile == execution['production_profile'] and profile['revision'] == lease['production_revision']
            and profile['expected_pipeline'] == PIPELINE, 'Different final production profile')
    checked(execution['binding'])
    binding = validate_binding(profile, execution['binding']['path'])
    require(binding == execution['binding_contents'], 'Actual-PID help/launcher binding differs')
    rust = load_rust_reference(execution['rust_reference']['path'])
    require(rust == execution['rust_reference_contents'] and rust['compiler'] == execution['rustc'], 'Different Rust reference')
    reference = original_reference(execution, rust)
    require(set(execution['tools']) == set(TOOLS), 'Incomplete tool closure')
    git_tools = real.git_archive_hashes(execution['source_commit_at_start'],
                                     ['tools/research/incremental-compilation/' + name for name in TOOLS])
    tool_root = root.with_name(root.name + '-tools')
    require(execution['frozen_tools_directory'] == str(tool_root), 'Unexpected tool snapshot path')
    for name in TOOLS:
        tool = checked(execution['tools'][name])
        require(tool['sha256'] == digest(tool_root / name)
                == git_tools['tools/research/incremental-compilation/' + name], 'Changed committed/frozen tool: ' + name)
    require(execution['runner'] == execution['tools']['stage6_real_pair_bench.py']
            and digest(root / 'runner.py') == execution['runner']['sha256'] == manifest['tool_sha256']
            and manifest['tool_sha256'] == read(reference['manifest']['path'])['tool_sha256']
            and digest(root / 'stage6_bench.py') == manifest['timing_runner_sha256'] == execution['tools']['stage6_bench.py']['sha256']
            and digest(root / 'distribution_bench.py') == manifest['runner_sha256'] == execution['tools']['distribution_bench.py']['sha256'],
            'Runner or timing helper changed')
    require(manifest['source'] == execution['source_commit_at_start'] and manifest['cases'] == real.CASES
            and manifest['modes'] == ['O2'] and manifest['blocks'] == 6 and manifest['stdin_hex'] == real.INPUTS
            and manifest['semantic_cache'] is True and manifest['closed_executable'] is False
            and manifest['zil_args'] == FLAGS and manifest['cpus'] == [0, 1]
            and manifest['native_arguments'] == {'app': {'rust': ['--check'], 'zil': []}}, 'Real protocol changed')
    require(set(manifest['binaries']) == {'zil', 'rust'}
            and checked(manifest['binaries']['zil']) == checked(binding['launcher'])
            and checked(manifest['binaries']['rust']) == rust['compiler'], 'Actual binaries differ')
    expected = [execution['runner']['path'], '--zil', binding['launcher']['path'], '--rustc', rust['compiler']['path'],
                '--out', str(root), '--mode', 'O2', '--blocks', '6', '--semantic-cache']
    expected += ['--zil-arg=' + flag for flag in FLAGS]
    require(execution['command'][1:] == expected, 'Real runner invocation differs')
    sources = real.source_audit(root, manifest)
    for case in real.CASES:
        require(sources[case] == reference['cases'][case]['sources'], 'New corpus differs from df04 reference')
    return execution, lease, manifest, profile, reference, sources


def object_identity_audit(native):
    """Current producer/verification checks; cache efficiency is observed separately."""
    require(native['backend'] == 'llvm-api' and type(native['actual_units']) is int
            and 0 < native['actual_units'] <= 8 and native['requested_units'] == 8
            and 0 < native['jobs'] <= 2, 'Wrong actual backend/unit/job contract')
    require(len(native['object_reports']) == native['actual_units'], 'Incomplete object reports')
    for item in native['object_reports']:
        require(item['pipeline'] == PIPELINE and item['backend'] == 'llvm-api'
                and item['llvm_version'] == '23.1.4', 'Wrong actual object producer/pipeline')
        if item['input_kind'] == 'object-cache':
            require(item['cache']['state'] == 'hit' and item['producer_verified_before_after'] is True
                    and item['llvm_verification_this_request'] is False, 'Wrong cached verification boundary')
        else:
            require(item['input_kind'] == 'builder' and item['verify_before_after'] is True,
                    'New object lacks LLVM pre/post verification')
    require(isinstance(native['object_cache'], dict), 'Missing object-cache report')
    return native['object_cache']


def cache_state_observation(request, report, cold_keys):
    """Report expected-cache deviations without changing request validity or samples."""
    native, semantic = report['native_codegen'], report.get('disk_cache')
    cache, units, objects = native['object_cache'], native['actual_units'], native['object_reports']
    states = [item.get('cache', {}).get('state') for item in objects]
    keys = [item.get('cache', {}).get('action_key') for item in objects]
    preflight = [item.get('preflight_ns') for item in objects]
    issues = []
    if semantic != ('missing' if request == 'cold' else 'hit'):
        issues.append('semantic_state')
    if cache.get('requested') is not True or cache.get('enabled') is not True or cache.get('disabled_reason') is not None:
        issues.append('object_cache_availability')
    expected_hits = 0 if request == 'cold' else units
    if cache.get('hits') != expected_hits or cache.get('misses') != units - expected_hits:
        issues.append('expected_object_hits_misses')
    if cache.get('hits') != states.count('hit') or cache.get('misses') != states.count('miss'):
        issues.append('object_accounting')
    if any(type(cache.get(name)) is not int or cache[name] < 0 for name in ['hits', 'misses']):
        issues.append('object_counter_type')
    for name in ['corrupt', 'write_failures']:
        if type(cache.get(name)) is not int or cache[name] != 0:
            issues.append('object_' + name)
    if any(type(value) is not int or value <= 0 for value in preflight):
        issues.append('current_object_preflight')
    cache_preflight = cache.get('current_preflight_ns')
    if type(cache_preflight) is not int or cache_preflight < 0 or (states.count('hit') > 0 and cache_preflight == 0):
        issues.append('current_cache_preflight')
    if any(not isinstance(key, str) or not key for key in keys):
        issues.append('action_key_presence')
    if request == 'unchanged' and (cold_keys is None or keys != cold_keys):
        issues.append('unchanged_action_keys')
    return {'status': 'Expected' if not issues else 'Unexpected', 'anomalies': issues,
            'semantic_state': semantic, 'object_cache': cache, 'object_states': states,
            'action_keys': keys, 'cold_action_keys': cold_keys, 'object_preflight_ns': preflight,
            'fallback': native.get('fallback'), 'actual_units': units,
            'validity_effect': 'None. All raw timings and original validity remain in statistics.'}


def audit_rows(root, execution, lease, manifest, reference, sources):
    oracles = {}
    for case in real.CASES:
        folder = root / ('oracle-' + case)
        oracle = read(folder / 'record.json')
        expected_command = [manifest['binaries']['rust']['path'], str(root / ('source-' + case) / 'main.rs'),
                            '--edition=2024', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                            '-C', 'opt-level=0', '-o', str(folder / 'program')]
        require(oracle['command'] == expected_command
                and oracle['native_arguments'] == (['--check'] if case == 'app' else []), 'Fresh oracle command differs')
        valid = (oracle['exit'] == 0 and oracle['timeout'] is False and oracle.get('launch_error') is None
                 and real.native_valid(oracle['native']) and oracle['native'] == reference['cases'][case]['native'])
        require(oracle['oracle_valid'] is valid, 'Fresh oracle differs from complete original outcomes')
        oracles[case] = oracle
    rows = [json.loads(line) for line in (root / 'rows.jsonl').read_text().splitlines()]
    grid = [(case, 'O2', block, position, arm, request) for case in real.CASES for block in range(6)
            for position, arm in enumerate(['zil', 'rust'] if block % 2 == 0 else ['rust', 'zil'])
            for request in real.REQUESTS]
    require(len(rows) == len(grid) == 336, 'Incomplete real grid')
    audit, retained, history, cold_keys, cache_observations = [], [], {}, {}, []
    for row, expected in zip(rows, grid):
        key = tuple(row[name] for name in ['case', 'mode', 'block', 'position', 'arm', 'request'])
        require(key == expected, 'Unexpected real request order/identity')
        case, mode, block, position, arm, request = key
        command, run = real.expected_command(row, root, manifest)
        require(row['command'] == command and row['source_identity_sha256'] == sources[case]['source_identity_sha256'],
                'Request source or command differs')
        require(type(row['elapsed_ns']) is int and row['elapsed_ns'] > 0, 'Invalid elapsed duration')
        cpu, rss = row.get('cpu_seconds'), row.get('max_rss_kib')
        require(cpu is None or (type(cpu) in [int, float] and math.isfinite(cpu) and cpu >= 0), 'Invalid CPU metric')
        require(rss is None or (type(rss) is int and rss > 0), 'Invalid RSS metric')
        stdout, stderr = run / (request + '.stdout'), run / (request + '.stderr')
        require(stdout.is_file() and stderr.is_file(), 'Missing raw compiler output')
        valid = row['exit'] == 0 and row['timeout'] is False and row.get('launch_error') is None and oracles[case]['oracle_valid']
        if valid:
            require(cpu is not None and rss is not None, 'Successful compiler lacks resource metrics')
        cache, cache_state = None, None
        if arm == 'zil':
            try:
                report = read(stdout)
            except (ValueError, OSError):
                report = None
            if not isinstance(report, dict):
                valid = False
            else:
                for field, report_field in [('status', 'status'), ('verification_runs', 'verification_runs'),
                        ('native_codegen', 'native_codegen'), ('disk_cache', 'disk_cache'), ('queries', 'query_events')]:
                    require(row.get(field) == report.get(report_field), 'Raw current report differs: ' + field)
                valid = valid and report['status'] == 'Success' and report['verification_runs'] == 1
                if valid:
                    cache = object_identity_audit(report['native_codegen'])
                    cache_state = cache_state_observation(request, report, cold_keys.get((case, block)))
                    if request == 'cold':
                        cold_keys[case, block] = cache_state['action_keys']
                    cache_observations.append({'case': case, 'block': block, 'request': request, **cache_state})
        if valid:
            require(row.get('native_arguments') == (['--check'] if case == 'app' and arm == 'rust' else []),
                    'Native invocation differs')
            valid = real.native_valid(row.get('native')) and row['native'] == oracles[case]['native']
        sequence = (case, block, arm)
        prior = history.get(sequence, True)
        require(row['execution_valid'] is bool(valid) and row['sequence_history_valid'] is prior
                and row['valid'] is (bool(valid) and prior), 'Validity/history differs')
        history[sequence] = row['valid']
        if request == 'unchanged' and row['execution_valid']:
            program = run / 'program'
            require(program.stat().st_size == row['exe_bytes'] and digest(program) == row['exe_sha256'], 'Final ELF changed')
            retained.append({'case': case, 'block': block, 'arm': arm, 'identity': identity(program)})
        native = row.get('native_codegen') or {}
        audit.append({'case': case, 'mode': mode, 'block': block, 'position': position, 'arm': arm,
                      'request': request, 'valid': row['valid'], 'status': row.get('status'),
                      'elapsed_ns': row['elapsed_ns'], 'cpu_seconds': row.get('cpu_seconds'),
                      'max_rss_kib': row.get('max_rss_kib'), 'semantic_cache': row.get('disk_cache'),
                      'actual_units': native.get('actual_units'), 'fallback': native.get('fallback'),
                      'object_hits': cache.get('hits') if cache else None, 'object_misses': cache.get('misses') if cache else None,
                      'cache_state': cache_state,
                      'verification_runs': row.get('verification_runs'), 'native_outcomes': len(row.get('native', [])),
                      'stdout': identity(stdout), 'stderr': identity(stderr)})
    invalid = sum(not row['valid'] for row in rows)
    require(invalid == execution['invalid_requests'] == lease['invalid_requests'], 'Invalid-request count differs')
    return rows, {'requests': len(rows), 'invalid_requests': invalid, 'request_audit': audit,
                  'cache_state_observations': cache_observations,
                  'cache_state_summary': {'observed_zil_requests': len(cache_observations),
                      'unexpected_requests': sum(item['status'] == 'Unexpected' for item in cache_observations),
                      'boundary': 'Cache misses, availability, corruption/write failures, key changes and preflight telemetry are reported separately; they never exclude a valid slow observation.'},
                  'retained_final_artifacts': retained,
                  'oracle_native_outcomes': sum(len(value['native'] or []) for value in oracles.values()),
                  'native_outcomes': sum(len(row.get('native', [])) for row in rows),
                  'scope': 'Read-only audit. Original/fresh native bytes are recorded as JSON hex, not separate native process files. Final unchanged ELFs are rehashed; cold ELF hashes are historical. Actual-PID help loader binding is outside timing; no per-request DSO trace is claimed.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    root, output = args.window.resolve(), args.out.resolve()
    require(output != root and root not in output.parents, 'Analysis must be outside the measured root')
    output.mkdir(parents=True, exist_ok=False)
    result = {'status': 'Auditing', 'window': str(root), 'cells': [], 'groups': []}
    try:
        # Missing input must still leave a durable failure receipt in the new
        # outside-window output. Existing output rejection remains nonmutating.
        root = args.window.resolve(strict=True)
        require(root.is_dir(), 'Measured window is not a directory')
        require(output != root and root not in output.parents, 'Analysis must be outside the measured root')
        producer_paths = [Path(__file__), Path(real.__file__), HERE / 'stage6_production_identity_v5.py',
                          HERE / 'stage6_toolchain_runtime.py', HERE / 'stage6_merge_runtime.py', HERE / 'stage6_bench.py']
        for path in producer_paths:
            shutil.copy2(path, output / path.name)
        result['producers'] = [identity(path) for path in producer_paths]
        execution, lease, manifest, profile, reference, sources = setup(root)
        rows, audit = audit_rows(root, execution, lease, manifest, reference, sources)
        cells, groups = real.summarize(rows, 2000, 6302026)
        reported = read(root / 'summary.json')
        require(reported['rows'] == len(rows) and len(reported['cells']) == len(cells), 'Runner summary dimensions differ')
        for cell, original in zip(cells, reported['cells']):
            require(tuple(cell[key] for key in ['case', 'mode', 'request']) == tuple(original[key] for key in ['case', 'mode', 'request']),
                    'Runner summary cell order differs')
            for arm in ['zil', 'rust']:
                require(math.isclose(cell[arm + '_median_ms'], original['arms'][arm]['median_ms'], rel_tol=1e-12, abs_tol=1e-9)
                        and 6 - cell[arm + '_valid'] == original['arms'][arm]['failures'], 'Runner median/failure count differs')
            require((cell['zil_over_rust'] is None and original['zil_over_rust'] is None)
                    or (cell['zil_over_rust'] is not None and original['zil_over_rust'] is not None
                        and math.isclose(cell['zil_over_rust'], original['zil_over_rust'], rel_tol=1e-12, abs_tol=1e-15)),
                    'Runner point estimate differs')
        with (output / 'request-audit.jsonl').open('w') as stream:
            for row in audit.pop('request_audit'):
                stream.write(json.dumps(row) + '\n')
        cache_states = audit.pop('cache_state_observations')
        with (output / 'cache-state-audit.jsonl').open('w') as stream:
            for row in cache_states:
                stream.write(json.dumps(row) + '\n')
        write(output / 'cache-state-anomalies.json', [row for row in cache_states if row['status'] == 'Unexpected'])
        real.csv_write(output / 'cells.csv', cells, list(cells[0]))
        real.csv_write(output / 'all-adverse-cells.csv', [cell for cell in cells if cell['adverse_point_estimate']], list(cells[0]))
        real.csv_write(output / 'all-invalid-cells.csv', [cell for cell in cells if not cell['valid']], list(cells[0]))
        fields = ['case', 'mode', 'block', 'position', 'arm', 'request', 'valid', 'execution_valid',
                  'sequence_history_valid', 'elapsed_ns', 'cpu_seconds', 'max_rss_kib', 'exe_bytes', 'exe_sha256']
        real.csv_write(output / 'request-resources.csv', [{key: row.get(key) for key in fields} for row in rows], fields)
        result.update(status='AuditComplete', cells=cells, groups=groups, audit=audit, production_profile=profile,
                      reference=reference, sources=sources, execution_receipt=identity(root.with_name(root.name + '-execution.json')),
                      exclusive_lease=identity(root.with_name(root.name + '-exclusive-lease.json')),
                      inputs={name: identity(root / name) for name in
                              ['manifest.json', 'rows.jsonl', 'summary.json', 'runner.py', 'distribution_bench.py', 'stage6_bench.py']},
                      method={'implementation': 'Unmodified stage6_cli_analysis.summarize', 'draws': 2000, 'seed': 6302026,
                              'pairing': 'Each case shares even/odd-stratified six-block draws across both arms and both requests.',
                              'limits': 'Fourteen small real programs; independent Rust pairs establish behavior, not identical internal work. Cold means compiler caches, not OS page cache. No edit or monolithic-O2 claim.'})
        lines = ['# Final-production real336 audit', '',
                 'Explicit after + incremental-o2, trusted semantic/object caches, default exports; old 888 timings are not reused.', '',
                 '| Request group | ZIL/Rust GM | Paired 95% interval | Adverse / valid cells |',
                 '|---|---:|---:|---:|']
        for group in groups:
            lo, hi = group['paired_bootstrap_95'] or [float('nan'), float('nan')]
            ratio = group['geomean_ratio'] if group['geomean_ratio'] is not None else float('nan')
            lines.append(f'| {group["group"]} | {ratio:.6f} | [{lo:.6f}, {hi:.6f}] | {group["adverse_cells"]}/{group["valid_cells"]} |')
        lines += ['', audit['scope'], '',
                  f'Cache-state observations: {audit["cache_state_summary"]["unexpected_requests"]} unexpected out of '
                  f'{audit["cache_state_summary"]["observed_zil_requests"]} successful ZIL reports. '
                  'All such timings remain in the original validity/statistical population; this is not an all-hit assertion.',
                  '', result['method']['limits'], '']
        (output / 'REPORT.md').write_text('\n'.join(lines))
    except BaseException as error:
        result.update(status='AuditFailed', error=repr(error), cells=[], groups=[])
        raise
    finally:
        write(output / 'summary.json', result)
        print(json.dumps({'status': result['status'], 'summary': str(output / 'summary.json')}))


if __name__ == '__main__':
    main()

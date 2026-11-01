#!/usr/bin/env python3
"""Independent record audit for new production-quality generated/opaque CLI windows."""
import argparse
import datetime
import json
from pathlib import Path
import re
import shlex
import shutil
import sys

sys.dont_write_bytecode = True
import stage6_cli_analysis as common
import stage6_generated_cli_analysis as generated
import stage6_opaque_cli_analysis as opaque
from stage6_production_identity_v5 import PIPELINE, load_production_manifest, load_rust_reference

require, read, write = common.require, common.read, common.write
identity, checked, digest = common.identity, common.checked, common.digest
REQUESTS = generated.REQUESTS
OPAQUE_TOOLS = ['stage6_production_opaque_bench_v5.py', 'stage6_production_identity_v5.py', 'stage6_toolchain_runtime.py',
                'stage6_opaque_bench.py', 'stage6_merge_runtime.py', 'stage6_bench.py', 'stage6_production_ledger.py']


def binding_audit(record, profile):
    checked(record)
    binding = read(record['path'])
    require(binding['status'] == 'Pass' and binding['profile'] == profile['_manifest_identity'], 'Wrong production binding')
    launcher = checked(binding['launcher'])
    require(Path(launcher['path']) == Path(record['path']).parent / 'zil-production', 'Wrong launcher location')
    checked(binding['producer'])
    expected = '#!/bin/sh\nexport LD_LIBRARY_PATH=' + shlex.quote(str(Path(profile['llvm']['library']['path']).parent))
    expected += '\nexport ZIL_CLANG=' + shlex.quote(profile['clang']['path'])
    expected += '\nexec ' + shlex.quote(profile['compiler']['path']) + ' "$@"\n'
    require(Path(launcher['path']).read_text() == expected, 'Launcher does not execute the pinned production compiler/environment')
    process, audit = binding['process'], binding['loader_audit']
    require(read(Path(record['path']).parent / 'help.process.json') == process, 'Original help-process receipt differs')
    require(process.get('environment_overrides') == {
        'LD_LIBRARY_PATH': str(Path(profile['llvm']['library']['path']).parent), 'ZIL_CLANG': profile['clang']['path'],
        'LD_DEBUG': 'libs', 'LD_DEBUG_OUTPUT': str(Path(record['path']).parent / 'loader')}, 'Loader probe environment differs')
    require(process['command'] == [profile['compiler']['path'], '--help'] and process['exit'] == 0
            and process['timeout'] is False and type(process['pid']) is int and process['pid'] > 0,
            'Production compiler loader probe did not pass')
    log = Path(audit['log'])
    require(log == Path(record['path']).parent / ('loader.' + str(process['pid']))
            and digest(log) == audit['log_sha256'], 'Production loader log differs')
    libraries = sorted({str(Path(value.strip()).resolve(strict=True))
                        for value in re.findall(r'calling init:\s+([^\n]*libLLVM[^\n]*)', log.read_text())})
    require(audit['valid'] is True and libraries == audit['loaded_libraries'] == [profile['llvm']['library']['path']]
            and audit['loaded_library_sha256'] == profile['llvm']['library']['sha256'], 'Production PID loaded another library')
    require(audit['expected_library'] == profile['llvm']['library'], 'Loader expected-library identity differs')
    return binding


def setup(root, output):
    execution_path = root.with_name(root.name + '-execution.json')
    lease_path = root.with_name(root.name + '-exclusive-lease.json')
    execution, lease, manifest = read(execution_path), read(lease_path), read(root / 'manifest.json')
    window = execution['window']
    require(window in ['generated-default', 'generated-closed', 'opaque-default', 'opaque-closed'], 'Wrong production window')
    corpus, contract = window.split('-')
    count = 1380 if corpus == 'generated' else 360
    require(execution['kind'] == 'production-quality-cli-v5' and execution['status'] == 'CompletedRunner'
            and execution['exit'] == 0 and lease['status'] == 'Released', 'Window or exclusive lease did not complete')
    require(execution['planned_complete_cli_requests'] == lease['planned_cli_requests'] == lease['observed_requests'] == count,
            'Incomplete or excess planned window')
    require(execution['source_commit_at_start'] == lease['source_commit'], 'Benchmark checkout revision differs')
    require(lease['window'] == window and lease['out'] == execution['out'] == str(root)
            and lease['production_manifest'] == execution['production_manifest']
            and lease['rust_reference'] == execution['rust_reference'], 'Lease does not bind this exact window/production/reference')
    checked(execution['exclusive_lease_at_start'])
    active_lease = read(execution['exclusive_lease_at_start']['path'])
    require(active_lease['status'] == 'Active' and all(active_lease[key] == lease[key] for key in
            ['window', 'out', 'production_manifest', 'rust_reference', 'source_commit', 'production_revision', 'planned_cli_requests', 'started_utc']),
            'Frozen active lease differs from the released window')
    require(execution['observed_requests'] == execution['completed_requests'] == count
            and execution['interrupted_requests'] == execution['not_run_requests'] == execution['duplicate_request_keys']
            == execution['unexpected_request_keys'] == execution['attempted_incomplete_requests'] == 0
            and execution['attempted_requests'] == count and execution['raw_row_parse_errors'] == [], 'Coordinator grid incomplete')
    checked(execution['not_run'])
    checked(execution['attempted_incomplete'])
    require(read(execution['not_run']['path']) == [] and execution['owned_child_subreaper'] is True
            and read(execution['attempted_incomplete']['path']) == []
            and execution['post_runner_cleanup']['captured'] == []
            and execution['post_runner_cleanup']['all_owned_processes_reaped'] is True
            and execution['post_runner_cleanup']['no_running_descendants'] is True, 'Coordinator left descendants or unattempted requests')
    at = datetime.datetime.fromisoformat
    require(at(lease['started_utc']) <= at(execution['started_utc']) < at(execution['ended_utc']) <= at(lease['finished_utc']),
            'Execution outside exclusive lease')
    checked(execution['production_manifest'])
    profile = load_production_manifest(execution['production_manifest']['path'])
    require(profile == execution['production_profile'] and profile['revision'] == lease['production_revision'],
            'Production source/build identity differs')
    require(profile['expected_pipeline'] == PIPELINE == 'default<O2>|merge-functions=after|aliases=false',
            'Production pipeline must be exact canonical O2/after without a research suffix')
    binding = binding_audit(execution['binding'], profile)
    require(binding == execution['binding_contents'], 'Execution binding differs')
    checked(execution['rust_reference'])
    rust_reference = load_rust_reference(execution['rust_reference']['path'])
    require(rust_reference == execution['rust_reference_contents'] and rust_reference['compiler'] == execution['rustc'],
            'Compiler differs from the common frozen Rust reference')
    for name in ['coordinator', 'identity_tool', 'process_tool', 'ledger_tool']:
        checked(execution[name])
    require(manifest['blocks'] == 6 and manifest['requests'] == [list(value) for value in REQUESTS], 'Wrong block/request protocol')
    require(len(manifest['cpus']) == 2 and len(set(manifest['cpus'])) == 2, 'Wrong CPU affinity')
    require(manifest['closed_executable'] is (contract == 'closed'), 'Wrong export contract')
    command = [execution['runner']['path']]
    if corpus == 'generated':
        require(manifest['source'] == execution['source_commit_at_start'] and manifest['modes'] == ['O2'],
                'Wrong generated tool revision/mode')
        require(manifest['source_cache'] is False and manifest['semantic_cache'] is True
                and manifest['reference_semantic_cache'] is False, 'Wrong cache contract')
        require(manifest['zil_args'] == ['--function-merging', 'after', '--backend', 'llvm-api', '--incremental-o2']
                and manifest['reference_args'] == [], 'Wrong quality/Rust options')
        require(set(manifest['binaries']) == {'zil', 'rust'}
                and checked(manifest['binaries']['zil']) == checked(binding['launcher'])
                and checked(manifest['binaries']['rust']) == checked(execution['rustc']), 'Wrong actual binaries')
        require(digest(root / 'runner.py') == execution['runner']['sha256'] == manifest['tool_sha256'], 'Generated runner changed')
        git_source = common.git_archive_hashes(manifest['source'], ['tools/research/incremental-compilation/stage6_bench.py'])
        require(git_source['tools/research/incremental-compilation/stage6_bench.py'] == manifest['tool_sha256'],
                'Generated runner differs from its explicit source revision')
        command += ['--zil', binding['launcher']['path'], '--rustc', execution['rustc']['path'], '--out', str(root),
                    '--mode', 'O2', '--blocks', '6', '--semantic-cache', '--zil-arg=--function-merging', '--zil-arg=after',
                    '--zil-arg=--backend', '--zil-arg=llvm-api', '--zil-arg=--incremental-o2']
        if contract == 'closed':
            command += ['--closed-executable']
        source_ids, sources = generated.sources(root, output, manifest, 'rust')
        oracles = None
    else:
        require(manifest['kind'] == 'production-quality-opaque-cli-v5' and manifest['status'] == 'Completed'
                and manifest['control_only'] is False and manifest['source_commit'] == execution['source_commit_at_start'],
                'Wrong opaque epoch or incomplete runner')
        require(manifest['cases'] == opaque.CASES and manifest['seeds'] == opaque.SEEDS and manifest['rounds'] == opaque.ROUNDS,
                'Wrong fixed opaque corpus/native inputs')
        require(manifest['planned_requests'] == manifest['observed_requests'] == manifest['completed_requests'] == count
                and manifest['interrupted_requests'] == manifest['not_run_requests'] == manifest['attempted_incomplete_requests'] == 0
                and read(root / 'not-run.json') == [] and read(root / 'attempted-incomplete.json') == [],
                'Incomplete or interrupted opaque grid')
        require(manifest['production'] == profile['_manifest_identity'] and manifest['production_profile'] == profile
                and manifest['binding'] == execution['binding'] and manifest['binding_contents'] == binding,
                'Opaque runner is bound to another production identity')
        require(manifest['incremental_o2'] is True and manifest['comparison'] == 'production-quality-rust-v5'
                and manifest['reference'] == 'rust' and manifest['arms'] == {'candidate': {'launcher': binding['launcher']}}
                and manifest['rustc'] == execution['rustc'] and manifest['rust_reference'] == execution['rust_reference']
                and manifest['rust_reference_contents'] == rust_reference, 'Wrong opaque arm/strategy identity')
        require(set(manifest['tools']) == set(OPAQUE_TOOLS), 'Incomplete opaque tool closure')
        paths = ['tools/research/incremental-compilation/' + name for name in OPAQUE_TOOLS]
        git_tools = common.git_archive_hashes(manifest['source_commit'], paths)
        for name in OPAQUE_TOOLS:
            require(digest(root / name) == manifest['tools'][name]['sha256'] == git_tools['tools/research/incremental-compilation/' + name]
                    and (root / name).stat().st_size == manifest['tools'][name]['bytes'], 'Frozen opaque tool differs: ' + name)
        require(execution['runner'] == manifest['tools']['stage6_production_opaque_bench_v5.py'], 'Opaque entry identity differs')
        command += ['--production', profile['_manifest_identity']['path'], '--binding', execution['binding']['path'],
                    '--rust-reference', execution['rust_reference']['path'], '--out', str(root), '--blocks', '6']
        if contract == 'closed':
            command += ['--closed-executable']
        require(manifest['invocation'] == command, 'Opaque actual invocation differs')
        sources, oracles = opaque.source_audit(root, output, manifest)
        source_ids = None
    require(execution['command'][1:] == command, 'Coordinator command differs')
    return execution, lease, manifest, profile, binding, corpus, source_ids, sources, oracles


def opaque_command(root, manifest, row):
    # Independent construction; do not call the producer's command builder.
    case, block, position, arm = [row[key] for key in ['case', 'block', 'position', 'arm']]
    run = root / f'{case}-{block}-{position}-{arm}'
    source = root / ('source-' + case) / ('main.rs' if arm == 'rust' else 'main.zil')
    exe = run / 'program'
    if arm == 'rust':
        command = [manifest['rustc']['path'], str(source), '--edition=2024', '-A', 'warnings', '-C', 'opt-level=2',
                   '-C', 'overflow-checks=yes', '-C', 'panic=abort', '-C', 'codegen-units=8',
                   '-C', 'incremental=' + str(run / 'incremental'), '-o', str(exe)]
    else:
        command = [manifest['arms']['candidate']['launcher']['path'], 'build', str(source), '--opt', 'O2',
                   '--codegen-units', '8', '--codegen-jobs', '2', '--function-merging', 'after', '--semantic-cache',
                   str(run / 'semantics'), '--object-cache', str(run / 'objects'), '--json', '-o', str(exe),
                   '--backend', 'llvm-api', '--incremental-o2']
        if manifest['closed_executable']:
            command.append('--closed-executable')
    return command, run, source, exe


def opaque_rows(root, output, manifest, lease, sources, oracles):
    rows = [json.loads(line) for line in (root / 'rows.jsonl').read_text().splitlines()]
    grid = [(case, block, position, arm, request, edit) for case in opaque.CASES for block in range(6)
            for position, arm in enumerate(['candidate', 'rust'] if block % 2 == 0 else ['rust', 'candidate'])
            for request, edit in REQUESTS]
    require(len(rows) == len(grid) == 360, 'Incomplete or excess opaque row grid')
    history, statuses, semantics, audited = {}, {}, {}, []
    native_count = verify_count = hits = misses = retained_elfs = 0
    for row, key in zip(rows, grid):
        require(tuple(row[field] for field in ['case', 'block', 'position', 'arm', 'request', 'edit']) == key, 'Opaque row order differs')
        case, block, position, arm, request, edit = key
        command, run, source, exe = opaque_command(root, manifest, row)
        require(row['command'] == command, 'Opaque actual compiler options differ')
        require(row['source'] == dict(sources[case + '/' + str(edit)]['files'][source.name], path=str(source)),
                'Opaque historical input identity differs')
        require(type(row['elapsed_ns']) is int and row['elapsed_ns'] > 0
                and (row.get('max_rss_kib') is None or row['max_rss_kib'] > 0), 'Invalid timing/resources')
        stdout, stderr = run / (request + '.stdout'), run / (request + '.stderr')
        require(stdout.is_file() and stderr.is_file(), 'Missing compiler output')
        compiler_ok = type(row.get('exit')) is int and row['exit'] == 0 and row.get('timeout') is False and 'launch_error' not in row
        report, cache, report_problem = None, None, None
        if arm == 'candidate':
            try:
                report = read(stdout)
                require(isinstance(report, dict), 'Malformed compiler report')
            except (ValueError, KeyError, TypeError) as error:
                report_problem = repr(error)
            if report_problem is None:
                for field in ['status', 'verification_runs']:
                    require(row.get(field) == report.get(field), 'Raw/report field differs: ' + field)
                status = case + '/' + str(report.get('status'))
                statuses[status] = statuses.get(status, 0) + 1
                semantic = request + '/' + str(report.get('disk_cache'))
                semantics[semantic] = semantics.get(semantic, 0) + 1
                if report.get('status') != 'Success' or report.get('verification_runs') != 1:
                    report_problem = 'Current compiler success/Verify failed'
                else:
                    try:
                        require('fallback' in report['native_codegen'] and
                                (report['native_codegen']['fallback'] is None or isinstance(report['native_codegen']['fallback'], str)),
                                'Missing or malformed explicit fallback outcome')
                        # Empty suffix means the exact production canonical
                        # pipeline. No old pair identity is invented or altered.
                        cache = common.object_audit(report['native_codegen'], '')
                    except (ValueError, KeyError, TypeError) as error:
                        report_problem = repr(error)
                    if report_problem is None:
                        require(row.get('native_codegen') == report['native_codegen'], 'Retained native report differs')
                        verify_count += 1
                        hits += cache['hits']
                        misses += cache['misses']
            if report_problem:
                require('report_error' in row, 'Unrecorded independent report rejection')
                compiler_ok = False
            else:
                require('report_error' not in row, 'Producer rejected a report accepted by audit')
        artifact, native = row.get('executable'), row.get('native')
        if artifact is not None:
            require(compiler_ok and artifact['path'] == str(exe) and re.fullmatch(r'[0-9a-f]{64}', artifact['sha256'])
                    and type(artifact['bytes']) is int and artifact['bytes'] > 0, 'Invalid historical ELF record')
            if request == 'restore':
                checked(artifact)
                retained_elfs += 1
        native_ok = isinstance(native, list) and len(native) == 5
        if native is not None:
            require(native_ok and artifact is not None, 'Incomplete native oracle evidence')
            for outcome, seed in zip(native, opaque.SEEDS):
                expected = oracles[case][str(edit)][str(seed)]
                require(outcome['command'] == [str(exe)] and outcome['input_hex'] == bytes([seed]).hex()
                        and outcome['expected'] == expected, 'Native input/oracle differs')
                actual_out = (run / f'{request}-seed-{seed}.stdout').read_bytes()
                actual_err = (run / f'{request}-seed-{seed}.stderr').read_bytes()
                require(outcome['stdout_hex'] == actual_out.hex() and outcome['stderr_hex'] == actual_err.hex(), 'Native raw output differs')
                valid = (type(outcome.get('exit')) is int and outcome['exit'] == 0 and outcome.get('timeout') is False
                         and 'error' not in outcome and actual_err == b'' and actual_out == expected.to_bytes(4, 'little'))
                require(outcome['valid'] is valid, 'Native validity differs')
                native_ok = native_ok and valid
                native_count += 1
        valid = compiler_ok and artifact is not None and native_ok
        prior = history.get((case, block, arm), True)
        require(row['execution_valid'] is bool(valid) and row['sequence_history_valid'] is prior
                and row['valid'] is (bool(valid) and prior), 'Opaque execution/cache-history validity differs')
        history[case, block, arm] = row['valid']
        audited.append({'case': case, 'block': block, 'position': position, 'arm': arm, 'request': request, 'edit': edit,
                        'execution_valid': bool(valid), 'sequence_history_valid': prior, 'comparison_valid': row['valid'],
                        'status': row.get('status'), 'exit': row.get('exit'), 'timeout': row.get('timeout'),
                        'report_error': row.get('report_error'), 'independent_report_error': report_problem,
                        'source': row['source'], 'executable': artifact, 'native_outcomes': len(native) if native else 0,
                        'elapsed_ns': row['elapsed_ns'], 'cpu_seconds': row.get('cpu_seconds'), 'max_rss_kib': row.get('max_rss_kib'),
                        'semantic_cache': report.get('disk_cache') if isinstance(report, dict) else None,
                        'verification_runs': row.get('verification_runs'), 'object_hits': cache['hits'] if cache else None,
                        'object_misses': cache['misses'] if cache else None, 'stdout': identity(stdout), 'stderr': identity(stderr)})
    invalid = sum(not row['valid'] for row in rows)
    require(invalid == lease['invalid_requests'], 'Lease invalid count differs')
    with (output / 'request-audit.jsonl').open('w') as stream:
        for row in audited:
            stream.write(json.dumps(row) + '\n')
    return rows, {'requests': len(rows), 'valid_requests': len(rows) - invalid, 'invalid_requests': invalid,
                  'native_outcomes': native_count, 'current_successful_verify_requests': verify_count,
                  'directly_rehashed_final_restore_elfs': retained_elfs, 'status_counts': statuses,
                  'semantic_cache_counts': semantics, 'object_hits': hits, 'object_misses': misses}


def fallback_audit(rows, root, output, corpus):
    records, counts = [], {}
    for row in rows:
        if row['arm'] == 'rust':
            continue
        run_name = (f'{row["case"]}-O2-{row["block"]}-{row["position"]}-{row["arm"]}' if corpus == 'generated'
                    else f'{row["case"]}-{row["block"]}-{row["position"]}-{row["arm"]}')
        try:
            report = read(root / run_name / (row['request'] + '.stdout'))
            native = report.get('native_codegen') or {}
            require(isinstance(native, dict), 'Malformed native report')
        except (OSError, ValueError, TypeError, AttributeError):
            native = {}
        fallback_present = 'fallback' in native
        if row.get('execution_valid') is True:
            require(fallback_present and (native['fallback'] is None or isinstance(native['fallback'], str)),
                    'Successful native report must explicitly record fallback or null')
        fallback = native.get('fallback')
        record = {key: row.get(key) for key in ['case', 'block', 'position', 'arm', 'request', 'status', 'valid', 'execution_valid', 'sequence_history_valid']}
        record.update(actual_units=native.get('actual_units'), requested_units=native.get('requested_units'),
                      jobs=native.get('jobs'), fallback_present=fallback_present, fallback=fallback)
        records.append(record)
        key = json.dumps({'status': row.get('status'), 'comparison_valid': row['valid'],
                          'actual_units': native.get('actual_units'), 'fallback_present': fallback_present, 'fallback': fallback}, sort_keys=True)
        counts[key] = counts.get(key, 0) + 1
    fields = ['case', 'block', 'position', 'arm', 'request', 'status', 'valid', 'execution_valid', 'sequence_history_valid',
              'actual_units', 'requested_units', 'jobs', 'fallback_present', 'fallback']
    common.csv_write(output / 'partition-and-fallback-requests.csv', records, fields)
    return {'records': len(records), 'counts': counts,
            'scope': 'All ZIL requests remain included, including whole-module fallbacks and budget refusals. Actual unit count and complete fallback text are observations; no fallback is renamed partition success.'}


def verify_summary(rows, cells, recorded, corpus):
    require(recorded['rows'] == len(rows) and len(recorded['cells']) == len(cells), 'Original summary dimensions differ')
    if corpus == 'opaque':
        require(recorded['failures'] == sum(not row['valid'] for row in rows), 'Opaque summary failure count differs')
    for cell, old in zip(cells, recorded['cells']):
        require((cell['case'], cell['request']) == (old['case'], old['request']), 'Summary order differs')
        for arm, label in [('zil' if corpus == 'generated' else 'candidate', 'candidate'), ('rust', 'reference')]:
            group = old['arms'][arm]
            require(common.math.isclose(cell[label + '_median_ms'], group['median_ms'], rel_tol=1e-12, abs_tol=1e-9), 'Summary median differs')
            require(cell[label + '_median_peak_rss_kib'] == group['max_rss_kib' if corpus == 'generated' else 'median_max_rss_kib'], 'Summary peak RSS differs')
            valid_count = 6 - group['failures'] if corpus == 'generated' else group['valid']
            require(cell[label + '_valid'] == valid_count, 'Summary validity count differs')
            values = [row['elapsed_ns'] / 1e6 for row in rows if (row['case'], row['request'], row['arm']) == (cell['case'], cell['request'], arm)]
            require(len(values) == len(group['values_ms']) == 6 and all(common.math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-9)
                    for a, b in zip(values, group['values_ms'])), 'Summary timing array differs')
        point, original = cell['candidate_over_reference'], old['zil_over_rust' if corpus == 'generated' else 'candidate_over_rust']
        require((point is None and original is None) or (point is not None and original is not None
                and common.math.isclose(point, original, rel_tol=1e-12, abs_tol=1e-15)), 'Summary ratio differs')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bootstrap', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=6302026)
    args = parser.parse_args()
    require(args.bootstrap >= 100, 'Use at least 100 exploratory bootstrap draws')
    root, output = args.window.resolve(strict=True), args.out.resolve()
    require(output != root and root not in output.parents, 'New analysis must be outside measured input')
    output.mkdir(parents=True, exist_ok=False)
    producer_paths = [Path(__file__), Path(common.__file__), Path(generated.__file__), Path(opaque.__file__),
                      Path(__file__).with_name('stage6_production_identity_v5.py')]
    for path in producer_paths:
        shutil.copy2(path, output / path.name)
    result = {'status': 'Auditing', 'producers': [identity(path) for path in producer_paths], 'window': str(root)}
    try:
        execution, lease, manifest, profile, binding, corpus, source_ids, sources, oracles = setup(root, output)
        if corpus == 'generated':
            # Reuse its independent row/history/oracle checks with the exact
            # canonical production pipeline suffix. This is not an old pair.
            rows, audit = generated.audit_rows(root, output, manifest, lease, 'rust',
                                               {'candidate': {'pipeline_identity_tag': ''}}, source_ids)
            cells, groups = generated.summarize(rows, [case['name'] for case in manifest['cases']], 'rust', args.bootstrap, args.seed)
            cells = [{('candidate' + key[3:] if key.startswith('zil') else key): value for key, value in cell.items()} for cell in cells]
            audit.pop('scope', None)
        else:
            rows, audit = opaque_rows(root, output, manifest, lease, sources, oracles)
            cells, groups = opaque.summarize(rows, 'rust', args.bootstrap, args.seed)
        verify_summary(rows, cells, read(root / 'summary.json'), corpus)
        require(execution['invalid_requests'] == audit['invalid_requests'], 'Coordinator invalid count differs')
        fallbacks = fallback_audit(rows, root, output, corpus)
        fields = list(cells[0])
        common.csv_write(output / 'cells.csv', cells, fields)
        common.csv_write(output / 'all-adverse-cells.csv', [cell for cell in cells if cell['adverse_point_estimate']], fields)
        common.csv_write(output / 'all-invalid-cells.csv', [cell for cell in cells if not cell['valid']], fields)
        resource_fields = ['case', 'block', 'position', 'arm', 'request', 'edit', 'status', 'valid', 'execution_valid',
                           'sequence_history_valid', 'elapsed_ns', 'cpu_seconds', 'max_rss_kib']
        common.csv_write(output / 'request-resources.csv', [{field: row.get(field) for field in resource_fields} for row in rows], resource_fields)
        audit['scope'] = ('Read-only original records: no compiler or native program executed. Production source/build/ELF/DSO and a fresh window PID loader binding are checked; no per-request loader tracing is claimed. Historical timings/resources/exit/timeout remain row observations. Generated-source edits are frozen-generator reproductions, whereas opaque variants are original retained bytes. Generated rows contain no historical ELF hashes; opaque historical ELF identities are row records with only final restore artifacts rehashed. Rust incremental contents are neither read nor claimed as intermediate-state snapshots.')
        result.update(status='AuditComplete', corpus=corpus, strategy='quality-incremental-' + execution['window'].split('-')[1],
                      benchmark_revision=execution['source_commit_at_start'], production_profile=profile,
                      inputs={name: identity(root / name) for name in ['manifest.json', 'rows.jsonl', 'summary.json']},
                      execution_receipt=identity(root.with_name(root.name + '-execution.json')),
                      exclusive_lease=identity(root.with_name(root.name + '-exclusive-lease.json')),
                      binding=execution['binding'], source_inventory=sources, audit=audit, fallbacks=fallbacks,
                      cells=cells, groups=groups,
                      method={'point_estimate': 'Ratio of arm six-block medians for each fixed case/request.',
                              'bootstrap': 'Exploratory paired percentile bootstrap; independently sample three forward and three reverse blocks; share draws across both arms/all five requests within each fixed case. Cases are not resampled.',
                              'draws': args.bootstrap, 'seed': args.seed, 'confidence_level': 0.95,
                              'aggregation': 'Equal geometric-mean weight per valid fixed cell; complete invalid/adverse tables retained. Conditional valid-subset GM is not complete-corpus success or a random workload-population inference.',
                              'limits': 'Exclusive window on one machine and fixed corpus; cold compiler cache is not a cold OS page cache. Full CLI includes frontend/Verify/cache/backend/linking; source and native oracle work are outside the timer. Native correctness is separate from runtime quality. wait4 peak RSS is not aggregate system memory.'})
        lines = ['# Independent production quality CLI audit', '',
                 f'Strategy `{result["strategy"]}`, corpus `{corpus}`: {audit["requests"]} requests, {audit["valid_requests"]} valid, {audit["invalid_requests"]} invalid.',
                 f'Production revision `{profile["revision"]}`; benchmark revision `{result["benchmark_revision"]}`.', '',
                 '| Requests | Candidate/Rust GM | Exploratory paired 95% | Adverse / valid cells | Invalid cells |',
                 '|---|---:|---:|---:|---:|']
        for group in groups:
            low, high = group['paired_bootstrap_95'] or [float('nan'), float('nan')]
            ratio = group['geomean_ratio'] if group['geomean_ratio'] is not None else float('nan')
            lines.append(f'| {group["group"]} | {ratio:.6f} | [{low:.6f}, {high:.6f}] | {group["adverse_cells"]}/{group["valid_cells"]} | {group["invalid_cells"]} |')
        lines += ['', 'Ratios above one are slower for the candidate. All cells and fallbacks remain visible; failed cells have no speed ratio.',
                  '', audit['scope'], '', fallbacks['scope'], '', result['method']['limits'], '']
        (output / 'REPORT.md').write_text('\n'.join(lines))
    except BaseException as error:
        result.update(status='AuditFailed', error=repr(error))
        raise
    finally:
        write(output / 'summary.json', result)
        print(json.dumps({'status': result['status'], 'summary': str(output / 'summary.json')}))


if __name__ == '__main__':
    main()

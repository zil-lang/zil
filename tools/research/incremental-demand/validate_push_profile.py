#!/usr/bin/env python3
"""Qualify frozen push/3ec7 scalar diagnostic profile records; never run a compiler.

This is deliberately separate from the historical 990/producer-reuse validator.
Historical executable SHA values are compared as recorded, not rehashed from
an overwritten program path. Complete archived LLVM and mapped DSO bytes are
rehashed. This single profiled candidate block is not a formal timing result.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types

SOURCE = '25c8607ab77584e423605793bbcb553ff8099330'
BASELINE = '3ec7e8a960ca69368cd9a90f1a15ac063fec5a02'
BASELINE_BINARY = '1b9abd96de7989b4b9e01bc37ccbee54e5984617028988dffd5d00d516c73405'
CANDIDATE_BINARY = '50c316a9de2a6f422147ee3543ca32fde15979df7042dd61f8e440bf38336801'
CANDIDATES = {SOURCE: CANDIDATE_BINARY,
              '328eb392d8d88148a75a743d899b43aebae73587':
              'e7d8b194edad4dca5db8d1f2c3d38fa2c0ba18420bc6d7eb9e9e4b2f00c8dc5a',
              '03b078b5799fe44d98855f26279f0f09e1d25bf5':
              'd0d6b76c543148f44b3856cd41f4317b1da5e589a7735ec814c5bbdebb75630c'}
GENERATOR = 'tools/research/reactive-query-graph/native_workloads.py'
RUNNER = 'tools/research/reactive-query-graph/native_measure.py'
HELPER = 'tools/research/incremental-demand/push_counters.py'
REQUESTS = ['cold', 'unchanged', 'private', 'multi', 'broad', 'positions', 'error', 'recovery', 'restore']
LIBRARIES = {'baseline': '/workspace/.zil-environment/llvm-23.1.4/lib',
             'candidate': '/workspace/.zil-environment/llvm-function-products-v15/lib'}
DSOS = {'baseline': '2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250',
        'candidate': '200a8dbefbb8dc13dfff539a72969378c57feb7a8f2d36c635589507c4376461'}


def require(value, context):
    if not value:
        raise ValueError(context)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def lines(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def pairs():
    for index, label in enumerate(REQUESTS):
        parent = None if label == 'cold' else ('error' if label == 'recovery' else 'private' if label == 'restore' else 'cold')
        if parent is not None:
            yield index, parent, False, None
        yield index, label, True, parent


def check_arguments(arguments, source=SOURCE):
    expected = {'case': 'scalar', 'modules': 20, 'functions': 20, 'steps': 16,
                'blocks': 1, 'request': None, 'mode': 'O2', 'backend': 'llvm-api',
                'cache': 'all', 'source_cache': False, 'closed_executable': False,
                'codegen_units': 16, 'codegen_jobs': 1, 'capacity': 256,
                'extra_arg': [], 'baseline_extra_arg': [],
                'candidate_extra_arg': ['--push-reuse', '--profile-compilation'],
                'timeout': 180, 'native_timeout': 10, 'sample_ms': 10}
    for key, value in expected.items():
        require(arguments.get(key) == value, f'fixed argument differs: {key}')
    for arm, library in LIBRARIES.items():
        require(arguments[arm + '_llvm_library_path'] == library, f'{arm} library argument')
    require(arguments['baseline_source'] == BASELINE and arguments['candidate_source'] == source, 'argument source pins')


def expected_controller_command(repo, raw, arguments):
    command = ['python3', str(repo/RUNNER)]
    for name in ['baseline', 'candidate', 'baseline_source', 'candidate_source',
                 'baseline_llvm_library_path', 'candidate_llvm_library_path']:
        command += ['--'+name.replace('_', '-'), arguments[name]]
    command += ['--out', str(raw)]
    for name in ['case', 'modules', 'functions', 'steps', 'blocks', 'mode', 'backend',
                 'cache', 'codegen_units', 'codegen_jobs', 'capacity']:
        command += ['--'+name.replace('_', '-'), str(arguments[name])]
    return command + ['--candidate-extra-arg=--push-reuse',
                      '--candidate-extra-arg=--profile-compilation']


def check_controller_record(record, repo, raw, arguments, source=SOURCE):
    require(record['source'] == source and record['binary_sha256'] == CANDIDATES[source], 'controller source/binary')
    require(record['command'] == expected_controller_command(repo, raw, arguments), 'complete controller argv')


def controller_status(controller_exit, persisted_exit):
    require(persisted_exit == 0, 'persisted helper failed')
    require(controller_exit is None or type(controller_exit) is int and controller_exit == 0,
            'external controller failed')
    return {'controller_exit': controller_exit, 'persisted_helper_exit': persisted_exit,
            'controller_agreement': None if controller_exit is None else True,
            'timing_window_verified': controller_exit is not None}


def parse_controller_exit(value):
    return None if value == 'unknown' else int(value)


def qualify(repo, raw, build_path, controller_exit, source=SOURCE):
    manifest, rows, summary, build = read(raw/'manifest.json'), lines(raw/'rows.jsonl'), read(raw/'summary.json'), read(build_path)
    a = manifest['arguments']
    require(source in CANDIDATES, 'unsupported source pin')
    candidate_binary_sha256 = CANDIDATES[source]
    check_arguments(a, source)
    controller = controller_status(controller_exit, int(raw.with_suffix('.exit').read_text()))
    require(build['exit_code'] == 0, 'build failure')
    require(manifest['requests'] == REQUESTS and manifest['source_shas'] == {'baseline': BASELINE, 'candidate': source}, 'manifest source/request pins')
    require(build['source'] == build['source_after'] == source, 'build source binding')
    git = lambda *args: subprocess.check_output(['git', *args], cwd=repo)
    require(build['tree'] == git('rev-parse', source + '^{tree}').decode().strip(), 'source tree binding')
    require(build['command'] == ['cargo', 'build', '--locked', '--release', '-p', 'zil-cli', '--features', 'native-products'], 'build command')
    require(build['sha256'] == candidate_binary_sha256, 'fixed build binary')
    require(file_sha(build_path.with_suffix('.log')) == build['log_sha256'] and int(build_path.with_suffix('.exit').read_text()) == 0, 'build log/exit receipt')
    require(file_sha(Path(build['binary'])) == build['sha256'], 'actual build receipt binary path/hash')
    require(build['llvm_library']['sha256'] == DSOS['candidate'], 'build LLVM DSO')
    for arm, expected in [('baseline', BASELINE_BINARY), ('candidate', candidate_binary_sha256)]:
        binary = manifest['binaries'][arm]
        require(binary['path'] == a[arm] and binary['sha256'] == expected, f'{arm} binary metadata')
        require(file_sha(Path(binary['path'])) == expected, f'{arm} actual binary hash')
    require(a['candidate'] == build['binary'] and a['out'] == str(raw), 'binary/output binding')
    tool_hashes, pinned = {}, {}
    for path, name, key in [(RUNNER, 'native_measure.py', 'runner_sha256'),
                            (GENERATOR, 'native_workloads.py', 'generator_sha256'),
                            (HELPER, 'push_counters.py', 'llvm_comparison_helper_sha256')]:
        data = git('show', source + ':' + path)
        require((raw/name).read_bytes() == data and sha(data) == manifest[key], f'pinned Git tool bytes: {name}')
        pinned[name] = data
        tool_hashes[path] = sha(data)
    # Only execute bytes already checked against the frozen Git commit. The
    # generator's sole source-file read uses another pinned Git blob below.
    workload = types.ModuleType('native_workloads')
    workload.__file__ = str(repo/GENERATOR)
    exec(compile(pinned['native_workloads.py'], source+':'+GENERATOR, 'exec'), workload.__dict__)
    pc = types.ModuleType('pinned_push_counters')
    exec(compile(pinned['push_counters.py'], source+':'+HELPER, 'exec'), pc.__dict__)
    old = sys.modules.get('native_workloads')
    old_bytecode = sys.dont_write_bytecode
    sys.dont_write_bytecode = True  # Pinned runner imports its archived helper; never add raw pycache.
    sys.modules['native_workloads'] = workload
    try:
        runner = types.ModuleType('pinned_native_measure')
        runner.__file__ = str(raw/'native_measure.py')
        exec(compile(pinned['native_measure.py'], source+':'+RUNNER, 'exec'), runner.__dict__)
    finally:
        sys.dont_write_bytecode = old_bytecode
        if old is None:
            del sys.modules['native_workloads']
        else:
            sys.modules['native_workloads'] = old
    blob_cache = {}

    def blob(item):
        name = item.get('archive', item.get('path'))
        relative = Path(name)
        require(not relative.is_absolute() and '..' not in relative.parts, 'unsafe archive path')
        path = raw/relative
        require(not path.is_symlink() and path.resolve().is_relative_to(raw), 'escaped archive path')
        if name not in blob_cache:
            blob_cache[name] = (path.stat().st_size, file_sha(path))
        require(blob_cache[name] == (item['bytes'], item['sha256']), f'archive bytes/hash: {name}')

    environments = manifest['arm_runtime_environments']
    startups = {}
    for arm in ['baseline', 'candidate']:
        environment = environments[arm]
        expected_env = {'LD_LIBRARY_PATH': LIBRARIES[arm], 'ZIL_LLVM_LIB_DIR': LIBRARIES[arm],
                        'LD_PRELOAD': None, 'LD_AUDIT': None,
                        'ZIL_CLANG': '/workspace/.zil-environment/llvm-23.1.4/bin/clang',
                        'ZIL_LLVM_CONFIG': '/workspace/.zil-environment/llvm-23.1.4/bin/llvm-config', 'LLVM_CONFIG': None}
        require(environment == expected_env, f'{arm} runtime environment')
        work = raw/('block-0-'+arm)
        startup = startups[arm] = read(work/'startup.json')
        require(startup['command'] == runner.command(Path(a[arm]), raw/'source-0', work, types.SimpleNamespace(**a), arm), f'{arm} complete resident argv')
        require(startup['environment'] == environment and startup['ready'] == {'ready': True, 'protocol': 1, 'pid': startup['pid'], 'capacity_entries': 256, 'reset_after': 4096}, f'{arm} startup')
        for name in ['runtime-startup.json', 'runtime-final.json']:
            dsos = pc.inspect_runtime_receipt(read(work/name), environment, startup['pid'], raw, blob, runner.llvm_mappings)
            require(dsos == manifest['runtime_libraries'][arm] and [d['sha256'] for d in dsos] == [DSOS[arm]], f'{arm} startup/final DSO')
    require(len(rows) == 34, '34 rows')
    native_count, success_pairs, error_pairs, offset = 0, 0, 0, 0
    measured = {}
    log_expected = []
    previous = {}
    with tempfile.TemporaryDirectory(prefix='zil-profile-qualify-') as directory:
        fixture_root = Path(directory)
        io = fixture_root/'examples/mainline/resource_closures/io.zil'
        io.parent.mkdir(parents=True)
        io.write_bytes(git('show', source+':examples/mainline/resource_closures/io.zil'))
        workload.ROOT = fixture_root
        for ordinal, (index, label, measured_flag, parent) in enumerate(pairs(), 1):
            name = f'{index}-' + ('' if measured_flag else 'prepare-') + label
            files = workload.snapshot('scalar', label, 20, 20, 16)
            require(len(files) == 22, 'full 22-source fixture')
            hashes = {name: sha(content.encode()) for name, content in sorted(files.items())}
            changed = [name for name in sorted(files) if previous.get(name) != files[name]]
            identity = {'changed_files': changed, 'file_hashes': hashes,
                        'identity_sha256': sha(json.dumps(hashes, sort_keys=True).encode()),
                        'files': len(files), 'bytes': sum(len(s.encode()) for s in files.values()),
                        'lines': sum(len(s.splitlines()) for s in files.values())}
            require(read(raw/f'source-0-{name}.json') == identity, f'{name} source/parent identity')
            snapshot = raw/'snapshots'/label
            require({str(p.relative_to(snapshot)) for p in snapshot.rglob('*.zil')} == set(files), f'{label} source set')
            for filename, content in files.items():
                require((snapshot/filename).read_bytes() == content.encode(), f'{label}/{filename} pinned fixture')
            pair = rows[offset:offset+2]
            order = ['baseline', 'candidate'] if index % 2 == 0 else ['candidate', 'baseline']
            for position, (row, arm) in enumerate(zip(pair, order)):
                context = f'{name}/{arm}'
                expected = {'block': 0, 'request_index': index, 'request': label, 'measured': measured_flag,
                            'transition_from': parent, 'position': position, 'arm': arm, 'case': 'scalar', 'mode': 'O2',
                            'source_identity_sha256': identity['identity_sha256'], 'changed_files': changed,
                            'valid': True, 'paired_semantics_equal': True}
                require(all(row.get(k) == v for k, v in expected.items()), context+' row/history')
                work = raw/('block-0-'+arm)
                require(read(work/(name+'.response.json')) == row['resident'], context+' response bytes')
                runner.validate_response(row['resident'], ordinal)
                report = row['resident']['report']
                require(all(row[k] == value for k, value in runner.report_summary(row['resident']).items()), context+' report projection')
                require(set(report['source_files'].values()) == {str(raw/'source-0'/filename) for filename in files}, context+' current source locations')
                runtime = read(work/(name+'.runtime.json'))
                require(runtime == row['runtime'], context+' runtime receipt')
                dsos = pc.inspect_runtime_receipt(runtime, environments[arm], startups[arm]['pid'], raw, blob, runner.llvm_mappings)
                require(dsos == manifest['runtime_libraries'][arm], context+' runtime DSO epoch')
                for field in ['wall_ms', 'cpu_seconds', 'own_cpu_seconds', 'tree_peak_sampled_rss_sum_kib']:
                    require(type(row[field]) in (int, float) and math.isfinite(row[field]) and row[field] >= 0, context+' '+field)
                samples = read(work/(name+'.memory-samples.json'))
                require(len(samples) == row['memory_sample_count'] and samples, context+' memory sampling count')
                require(max(s['tree_rss_sum_kib'] for s in samples) == row['tree_peak_sampled_rss_sum_kib'], context+' sampled RSS maximum')
                if arm == 'candidate':
                    pc.inspect_report(report, mode='o2', cold=ordinal == 1, profiled=True)
                    if ordinal == 1:
                        require(report['types_push_work']['body_checks'] == 425 and report['frontend_stats']['initialization']['candidate_executed_functions'] == 425 and report['backend_work']['lowered_functions'] == 425, 'actual full 425 cold functions')
                if label == 'error':
                    require(report['status'] == 'SafeReject' and report['phase'] == 'types' and report['verification_runs'] == 0 and len(report['diagnostics']) == 1 and report['diagnostics'][0]['code'] == 'E-UNKNOWN-NAME' and report['diagnostics'][0]['span'] is not None, context+' safe error')
                    require(not any(k in row for k in ['native', 'executable_sha256', 'llvm_archive']), context+' failed request artifacts')
                else:
                    require(report['status'] == 'Success' and report['diagnostics'] == [] and report['verification_runs'] == 1 and report['query_events'].count({'phase': 'verify', 'disposition': 'executed'}) == 1, context+' independent current Verify')
                    require(len(row['native']) == 6, context+' six native outcomes')
                    for outcome, data in zip(row['native'], workload.inputs('scalar')):
                        oracle = workload.oracle('scalar', data, label, 20, 20, 16)
                        require(outcome == {'stdin_hex': data.hex(), 'expected': oracle, 'actual': oracle, 'valid': True}, context+' pinned native oracle')
                        native_count += 1
                    require(re.fullmatch('[0-9a-f]{64}', row['executable_sha256']) is not None, context+' recorded executable hash')
                    require(report['artifacts'] == {'executable': str(work/'program'), 'llvm': str(work/'program.ll')}, context+' publication paths')
                    objects = report['native_codegen']['object_reports']
                    require(len(objects) == 16, context+' actual CGU receipts')
                    require(all(o['backend'] == 'llvm-api' and o['pipeline'] == 'default<O2>' for o in objects), context+' actual backend/optimization')
                    for obj in objects:
                        if obj['cache']['state'] == 'hit':
                            require(obj['producer_verified_before_after'] is True and obj['llvm_verification_this_request'] is False, context+' cached producer verification scope')
                        else:
                            require(obj['verify_before_after'] is True, context+' current object producer verification')
                    blob(row['llvm_archive'])
                    llvm = (raw/row['llvm_archive']['path']).read_bytes()
                    require(sha(llvm) == row['llvm_text_sha256'] == row['llvm_archive']['sha256'], context+' raw LLVM hash')
                    require(pc.llvm_comparison_sha256(llvm) == row['llvm_comparison_sha256_excluding_identity_comment'], context+' strict LLVM comparison hash')
            require(runner.pair_validation(pair), name+' complete paired semantics')
            identity_flags = runner.codegen_identity(pair, 'O2', True)
            for row in pair:
                require(all(row[k] == v for k, v in identity_flags.items()), name+' codegen flags')
                require(row['paired_executable_equal'] == (label != 'error'), name+' recorded ELF equality flag')
            if label == 'error':
                error_pairs += 1
            else:
                require(pair[0]['executable_sha256'] == pair[1]['executable_sha256'] and identity_flags['paired_codegen_identity_valid'] is True, name+' full recorded ELF/strict LLVM equality')
                success_pairs += 1
            if measured_flag:
                measured[label] = {r['arm']: r for r in pair}
            log_expected.append({'block': 0, 'request': label, 'measured': measured_flag, 'pair_valid': True,
                                 'arms': {r['arm']: {'wall_ms': r['wall_ms'], 'status': r['status']} for r in pair}})
            previous = files
            offset += 2
    require(lines(raw.with_suffix('.log')) == log_expected, 'controller pair log')
    require(native_count == 180 and success_pairs == 15 and error_pairs == 2, 'native/pair counts')
    require({k: summary[k] for k in ['rows', 'measured_rows', 'invalid_rows', 'native_executions']} == {'rows': 34, 'measured_rows': 18, 'invalid_rows': 0, 'native_executions': 180}, 'summary counts')
    require(len(summary['cells']) == 9, 'all nine measured scenarios')
    for cell, label in zip(summary['cells'], REQUESTS):
        b, c = (measured[label][arm]['wall_ms'] for arm in ['baseline', 'candidate'])
        require(cell == {'request': label, 'valid_pairs': 1, 'expected_pairs': 1,
                         'pairs': [{'block': 0, 'baseline_ms': b, 'candidate_ms': c, 'candidate_over_baseline': c/b, 'saved_ms': b-c}],
                         'median_baseline_ms': b, 'median_candidate_ms': c,
                         'baseline_range_ms': [b, b], 'candidate_range_ms': [c, c],
                         'median_candidate_over_baseline': c/b, 'median_saved_ms': b-c}, label+' summary')
    command = read(raw.with_name(raw.name+'-command.json'))
    check_controller_record(command, repo, raw, a, source)
    return {'schema': 'push-profile-qualification-v1', 'valid': True,
            'source': source, 'tree': build['tree'], 'baseline': BASELINE,
            'candidate_binary_sha256': candidate_binary_sha256, 'baseline_binary_sha256': BASELINE_BINARY,
            'counts': {'rows': 34, 'measured': 18, 'parents': 16, 'pairs': 17, 'successful_pairs': 15, 'safe_error_pairs': 2, 'native_outcomes_recomputed': native_count, 'files': 22, 'functions': 425},
            'raw': str(raw), 'build_receipt': {'path': str(build_path), 'sha256': file_sha(build_path)},
            'raw_sha256': {name: file_sha(raw/name) for name in ['manifest.json', 'rows.jsonl', 'summary.json']},
            'pinned_tools_sha256': tool_hashes, 'checked_archived_blobs': len(blob_cache),
            'runtime_dso_sha256': DSOS, **controller,
            'historical_validator_incompatibility': {'path': 'tools/research/incremental-demand/profile_validate.py', 'sha256': sha(git('show', source+':tools/research/incremental-demand/profile_validate.py')), 'independently_reexecuted': False, 'reason': 'Historical validator explicitly requires baseline990 and producer-reuse; it is unchanged. Root observed its baseline assertion fail on this 3ec7/push profile.'},
            'scope': ['Single diagnostic block: candidate profiled, baseline unprofiled; no Rust or earlier candidate timing comparison.',
                      *(['External controller completion/continuous timing were not observed after environment reconnect; recorded quality may be checked, performance attribution is excluded.'] if controller_exit is None else []),
                      'ELF equality uses all historical recorded full-file SHA256 values; overwritten historical executable bytes were not physically rehashed.',
                      'Every archived LLVM blob was rehashed; only the unique exact second-line hex64 verified-fingerprint value is normalized, all other bytes must agree.',
                      'Original pinned runner removes output before each request; this diagnostic workflow differs from retaining historical output.',
                      'RSS is sampled process-tree RSS sum, may double-count shared pages and miss peaks; CPU is observed process accounting, not an exclusive phase cost.',
                      'Current stage receipts retain missing/unknown values; profile counters describe their recorded scopes, including first-attempt native work versus retry-inclusive wall time.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['repo', 'run-dir', 'build-receipt', 'output']:
        parser.add_argument('--'+name, required=True, type=Path)
    parser.add_argument('--source', default=SOURCE, choices=sorted(CANDIDATES))
    parser.add_argument('--controller-exit', required=True, type=parse_controller_exit, help='externally observed exit, or unknown after a lost controller; persisted helper exit remains required')
    args = parser.parse_args()
    result = qualify(args.repo.resolve(), args.run_dir.resolve(), args.build_receipt.resolve(), args.controller_exit, args.source)
    result['qualifier_sha256'] = file_sha(Path(__file__))
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({'valid': True, 'counts': result['counts'], 'receipt': str(args.output)}))


if __name__ == '__main__':
    main()

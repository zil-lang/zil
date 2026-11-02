#!/usr/bin/env python3
"""New production quality epoch: prepare five arms, then measure frozen programs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import traceback

from stage6_merge_runtime import CASES, SEEDS, generate
from stage6_toolchain_runtime import (clean_environment, command_run, executable_audits,
    identity, loader_audit, now, save, sha)
from stage6_production_identity_v5 import (PIPELINE, checked_identity,
    load_production_manifest, load_rust_reference, production_environment)

HERE = Path(__file__).resolve().parent
FORMAL = 'production-quality-five-arm-v5'
BOUNDARY = 'production-quality-boundary-8192-branch-v5'
ARM_OPTIONS = {
    'monolithic_after': ['--function-merging', 'after'],
    'monolithic_after_closed': ['--function-merging', 'after', '--closed-executable'],
    'quality_after': ['--function-merging', 'after', '--incremental-o2'],
    'quality_after_closed': ['--function-merging', 'after', '--incremental-o2', '--closed-executable'],
    'rust': [],
}
PINNED_HELPERS = {
    'stage6_toolchain_runtime.py': 'cc611392b17269b7c6a9bbbc72ac785735a30326d295e230792a8f5d90d3b75c',
    'stage6_merge_runtime.py': '569288f3152460e11463216ea998cd2621c6fac2afaf7931ba7ae7eb13652045',
}
SCRIPTS = [Path(__file__).name, 'stage6_production_identity_v5.py',
           'stage6_toolchain_runtime.py', 'stage6_merge_runtime.py', 'stage6_bench.py']


class TerminationRequested(BaseException):
    pass


def terminate_requested(number, _frame):
    raise TerminationRequested('received signal ' + str(number))


def protocol(profile):
    if profile not in [FORMAL, BOUNDARY]:
        raise ValueError('unknown quality runtime profile')
    cases = CASES if profile == FORMAL else [(8192, 'branch')]
    return {'cases': [[n, shape] for n, shape in cases], 'seeds': SEEDS, 'rounds': 20000,
            'blocks': 6 if profile == FORMAL else 0, 'builds': len(cases) * 5,
            'native_preflight_observations': len(cases) * 25,
            'formal_observations': 900 if profile == FORMAL else 0,
            'compile_timeout_seconds': 240, 'runtime_timeout_seconds': 60,
            'quality_expected_units': {f'{n}-{shape}': 3 if n == 512 else 8 for n, shape in cases}}


def check_helpers():
    for name, expected in PINNED_HELPERS.items():
        if sha(HERE / name) != expected:
            raise ValueError('frozen process/generator helper changed: ' + name)


def load_config(path):
    path = Path(path).resolve(strict=True)
    config = json.loads(path.read_text())
    if config.get('schema') != 1 or config.get('protocol') != protocol(config.get('profile')):
        raise ValueError('quality protocol changed')
    check_helpers()
    producer = checked_identity(config['producer'], path.parent)
    if producer != identity(__file__):
        raise ValueError('configuration was created by another runner revision')
    build = checked_identity(config['production_build'], path.parent)
    production = load_production_manifest(build['path'])
    if production != config['production']:
        raise ValueError('production compiler/source/library binding changed')
    rust_ref = checked_identity(config['rust_reference'], path.parent)
    rust = load_rust_reference(rust_ref['path'])
    if rust != config['rust']:
        raise ValueError('shared Rust reference changed')
    reference = checked_identity(config['reference_manifest'], path.parent)
    prior = json.loads(Path(reference['path']).read_text())
    if (prior['functional_smoke_only'] or prior['rounds'] != 20000 or prior['seeds'] != SEEDS or
            prior['cases'] != [f'{n}-{shape}' for n, shape in CASES] or
            prior['scripts']['stage6_merge_runtime.py']['sha256'] != PINNED_HELPERS['stage6_merge_runtime.py'] or
            next(arm['compiler'] for arm in prior['arms'] if arm['name'] == 'rust') != rust['compiler']):
        raise ValueError('not the frozen six-family source and Rust reference')
    tools = {name: checked_identity(config['tools'][name], path.parent) for name in ['clang', 'nm', 'size']}
    if tools['clang'] != production['clang'] or any(tools[name] != prior['tools'][name] for name in tools):
        raise ValueError('native audit/link tools differ from the frozen reference')
    expected_arms = [{'name': name, 'language': 'rust' if name == 'rust' else 'zil', 'options': options,
                      'compiler': rust['compiler'] if name == 'rust' else production['compiler']}
                     for name, options in ARM_OPTIONS.items()]
    if config['arms'] != expected_arms:
        raise ValueError('five-arm compiler/options/order changed')
    return config, production, tools, prior


def create_config(args):
    if args.out.exists():
        raise ValueError('configuration already exists; never overwrite')
    check_helpers()
    production = load_production_manifest(args.production_build)
    rust = load_rust_reference(args.rust_reference)
    prior_path = args.reference_prepared.resolve(strict=True) / 'manifest.json'
    prior = json.loads(prior_path.read_text())
    config = {'schema': 1, 'profile': BOUNDARY if args.boundary_8192_branch else FORMAL,
              'producer': identity(__file__), 'production_build': production['_manifest_identity'],
              'production': production, 'rust_reference': rust['_reference_identity'], 'rust': rust,
              'reference_manifest': identity(prior_path), 'tools': prior['tools'],
              'arms': [{'name': name, 'language': 'rust' if name == 'rust' else 'zil', 'options': options,
                        'compiler': rust['compiler'] if name == 'rust' else production['compiler']}
                       for name, options in ARM_OPTIONS.items()]}
    config['protocol'] = protocol(config['profile'])
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('x') as stream:
        stream.write(json.dumps(config, indent=2) + '\n')
    load_config(args.out)
    print(json.dumps({'status': 'Pass', 'configuration': identity(args.out),
                      'boundary': 'Identity/configuration only; no builds or runtime observations'}))


def expected_outputs(n, shape, rounds):
    values = [sum(((x + 64 + i % 97) % 1009 if shape == 'body' else
                   (x * 3 + i % 97) % 1009 if shape != 'branch' or x % 3 == 0 else
                   (x + i % 31) % 1009) for i in range(n)) for x in range(1009)]
    expected = {}
    for seed in SEEDS:
        total, x = 0, seed
        for _ in range(rounds):
            total = (total + values[x]) % 1000003
            x = (x + 17) % 1009
        expected[str(seed)] = total
    return expected


def native_control(executable, prefix, seed, expected):
    process = command_run([executable], prefix, clean_environment(), 60, stdin=bytes([seed]))
    stdout = Path(process['stdout_path']).read_bytes()
    stderr = Path(process['stderr_path']).read_bytes()
    return {'seed': seed, 'expected': expected, 'expected_hex': expected.to_bytes(4, 'little').hex(),
            'stdout_hex': stdout.hex(), 'stderr_hex': stderr.hex(), 'process': process,
            'elapsed_ns': process['elapsed_ns'],
            'valid': process['exit'] == 0 and not process['timeout'] and
                     stdout == expected.to_bytes(4, 'little') and not stderr}


def inspect_codegen(report, arm, expected_units, production):
    codegen = report['native_codegen']
    units, fallback = codegen['actual_units'], codegen['fallback']
    reports = codegen['object_reports']
    controls = {
        'success': report['status'] == 'Success', 'current_verify_once': report['verification_runs'] == 1,
        'actual_api': codegen['backend'] == 'llvm-api',
        'actual_units': type(units) is int and units > 0,
        'requested_units': codegen['requested_units'] == 8,
        'bounded_jobs': type(codegen['jobs']) is int and 0 < codegen['jobs'] <= 2,
        'all_object_receipts': type(units) is int and len(reports) == units,
        'llvm_pre_post_verified': bool(reports) and all(item['verify_before_after'] is True for item in reports),
        'exact_production_pipeline': bool(reports) and all(item['pipeline'] == PIPELINE for item in reports),
        'exact_llvm_version': bool(reports) and all(item['llvm_version'] == production['llvm']['version'] for item in reports),
        'structured_builder': bool(reports) and all(item['input_kind'] == 'builder' for item in reports),
        'fresh_codegen': codegen['object_cache']['requested'] is False and codegen['object_cache']['enabled'] is False,
    }
    quality = arm['name'].startswith('quality_')
    if quality:
        strategy_valid = units == expected_units and fallback is None
        realized = ('complete_closure_partition' if strategy_valid else
                    'explicit_whole_module_fallback' if units == 1 and isinstance(fallback, str) and fallback else
                    'unexpected_partition_result')
    else:
        strategy_valid, realized = units == 1, 'monolithic'
    return {'controls': controls, 'functional_valid': all(controls.values()),
            'strategy_valid': strategy_valid, 'realized_strategy': realized,
            'requested_units': codegen['requested_units'], 'actual_units': units,
            'expected_units': expected_units if quality else 1, 'jobs': codegen['jobs'],
            'fallback': fallback, 'native_codegen': codegen}


def prepare(args):
    args.out.mkdir(parents=True, exist_ok=False)
    args._owns_epoch = True
    manifest = {'schema': 1, 'status': 'Preparing', 'started_at_utc': now(), 'command': sys.argv,
                'scope': __doc__, 'sources': {}, 'expected': {},
                'compile_timing_boundary': 'All builds and native preflight controls are functional preparation; compile elapsed time and peak RSS are diagnostic only'}
    save(args.out / 'manifest.json', manifest)
    config, production, tools, prior = load_config(args.config)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    manifest.update(profile=config['profile'], protocol=config['protocol'], config=identity(args.config),
                    configuration=config, cpus=cpus,
                    scripts={name: identity(HERE / name) for name in SCRIPTS},
                    fallback_policy='Record full fallback and functional validity; an unexpected strategy prevents this fixed profile from becoming ready. Never rewrite expectations after observing a result.')
    (args.out / 'scripts').mkdir()
    for name in SCRIPTS:
        shutil.copy2(HERE / name, args.out / 'scripts' / name)
    shutil.copy2(args.config, args.out / 'configuration.json')
    builds, oracles = [], []
    save(args.out / 'builds.json', builds)
    save(args.out / 'native-oracles.json', oracles)
    for n, shape in config['protocol']['cases']:
        case = f'{n}-{shape}'
        args._active_operation = {'phase': 'source_generation', 'case': case}
        directory = args.out / case
        directory.mkdir()
        generate(directory, n, shape, config['protocol']['rounds'])
        source_ids = {name: identity(directory / name) for name in ['main.zil', 'main.rs']}
        if config['profile'] == FORMAL:
            for name, actual in source_ids.items():
                original = prior['sources'][case][name]
                if (actual['sha256'], actual['bytes']) != (original['sha256'], original['bytes']):
                    raise ValueError('generated formal source differs from the frozen six-family suite')
        manifest['sources'][case] = source_ids
        expected = expected_outputs(n, shape, config['protocol']['rounds'])
        if config['profile'] == FORMAL and expected != prior['expected'][case]:
            raise ValueError('independent output calculation differs from the frozen source oracle')
        manifest['expected'][case] = expected
        save(args.out / 'manifest.json', manifest)
        for arm in config['arms']:
            name = arm['name']
            args._active_operation = {'phase': 'build', 'case': case, 'arm': name}
            work = directory / name
            work.mkdir()
            exe = work / 'program'
            command = [arm['compiler']['path']]
            environment = clean_environment()
            if arm['language'] == 'rust':
                command += [str(directory / 'main.rs'), '--edition=2024', '-A', 'warnings',
                            '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                            '-C', 'codegen-units=8', '-o', str(exe)]
            else:
                command += ['build', str(directory / 'main.zil'), '--backend', 'llvm-api', '--opt', 'O2',
                            '--codegen-units', '8', '--codegen-jobs', '2', '--clang', tools['clang']['path'],
                            '--json', '-o', str(exe), *arm['options']]
                environment = production_environment(production)
                environment.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(work / 'loader'))
            process = command_run(command, work / 'build', environment, 240)
            build = {'case': case, 'arm': name, 'process': process, 'native_controls': 0,
                     'functional_valid': process['exit'] == 0 and not process['timeout'] and exe.is_file(),
                     'strategy_valid': True, 'realized_strategy': 'rust' if name == 'rust' else 'Unreported'}
            if arm['language'] == 'zil':
                build['loader_audit'] = loader_audit(work / 'loader', process, {'llvm': production['llvm']})
                build['functional_valid'] &= build['loader_audit']['valid']
                try:
                    report = json.loads(Path(process['stdout_path']).read_text())
                    inspected = inspect_codegen(report, arm, config['protocol']['quality_expected_units'][case], production)
                    build['functional_valid'] &= inspected.pop('functional_valid')
                    build.update(inspected)
                except (ValueError, KeyError, TypeError) as error:
                    build.update(functional_valid=False, strategy_valid=False, report_error=str(error))
            if build['functional_valid']:
                build['executable'] = identity(exe)
                build['audits'] = executable_audits(exe, work, tools)
                build['functional_valid'] &= build['audits']['valid']
            build.update(valid=False, stage='NativeControlsPending' if build['functional_valid'] else 'BuildFailed')
            builds.append(build)
            save(work / 'build-result.json', build)
            save(args.out / 'builds.json', builds)
            if build['functional_valid']:
                outcomes = []
                for seed in SEEDS:
                    args._active_operation = {'phase': 'native_preflight', 'case': case, 'arm': name, 'seed': seed}
                    row = native_control(str(exe), work / 'native-preflight' / str(seed), seed, expected[str(seed)])
                    row.update(case=case, arm=name, scope='Nonexclusive preparation oracle; excluded from formal runtime observations')
                    outcomes.append(row)
                    oracles.append(row)
                    save(args.out / 'native-oracles.json', oracles)
                    build['native_controls'] = len(outcomes)
                    save(args.out / 'builds.json', builds)
                build['native_controls'] = len(outcomes)
                build['functional_valid'] &= all(row['valid'] for row in outcomes)
            build['valid'] = build['functional_valid'] and build['strategy_valid']
            build['stage'] = 'Complete'
            save(work / 'build-result.json', build)
            save(args.out / 'builds.json', builds)
            print('PREPARE', case, name, 'functional=' + str(build['functional_valid']),
                  'strategy=' + str(build['strategy_valid']), 'fallback=' + repr(build.get('fallback')), flush=True)
            args._active_operation = None
    complete = len(builds) == config['protocol']['builds']
    functional = complete and all(build['functional_valid'] for build in builds) and len(oracles) == config['protocol']['native_preflight_observations']
    strategy = complete and all(build['strategy_valid'] for build in builds)
    ready = functional and strategy and config['profile'] == FORMAL
    manifest.update(status=('Prepared' if ready else 'BoundaryControlsComplete' if config['profile'] == BOUNDARY else 'PreparationFailed'),
                    finished_at_utc=now(), functional_valid=functional, strategy_valid=strategy,
                    builds=len(builds), native_preflight_observations=len(oracles),
                    failed_functional_builds=sum(not build['functional_valid'] for build in builds),
                    unexpected_strategies=sum(not build['strategy_valid'] for build in builds))
    save(args.out / 'manifest.json', manifest)
    save(args.out / 'prepared.json', {'ready': ready, 'manifest_sha256': sha(args.out / 'manifest.json'),
        'builds_sha256': sha(args.out / 'builds.json'), 'native_oracles_sha256': sha(args.out / 'native-oracles.json')})
    if not functional or not strategy:
        raise RuntimeError('functional failure or unexpected strategy; every attempt and fallback retained')


def measure(args):
    if (args.out / 'execution.json').exists() or (args.out / 'rows.jsonl').exists():
        raise ValueError('formal output already exists; never overwrite or resume silently')
    manifest = json.loads((args.out / 'manifest.json').read_text())
    marker = json.loads((args.out / 'prepared.json').read_text())
    expected = {'ready': True, 'manifest_sha256': sha(args.out / 'manifest.json'),
                'builds_sha256': sha(args.out / 'builds.json'), 'native_oracles_sha256': sha(args.out / 'native-oracles.json')}
    if marker != expected or manifest['profile'] != FORMAL:
        raise ValueError('only a complete valid six-family formal preparation can be measured')
    config, _, _, prior = load_config(args.config)
    if identity(args.config) != manifest['config'] or config != manifest['configuration']:
        raise ValueError('prepared configuration changed')
    lease_bytes = args.lease.read_bytes()
    lease = json.loads(lease_bytes)
    if (lease.get('status') != 'Active' or lease.get('window') != 'quality-runtime-900' or
            lease.get('profile') != FORMAL or lease.get('out') != str(args.out) or
            lease.get('configuration') != manifest['config'] or
            lease.get('prepared_manifest') != identity(args.out / 'manifest.json') or
            lease.get('prepared_marker') != identity(args.out / 'prepared.json') or
            lease.get('production_build') != config['production_build'] or
            lease.get('rust_reference') != config['rust_reference']):
        raise ValueError('an active exclusive lease must bind this exact production/runtime window')
    for name, recorded in manifest['scripts'].items():
        if identity(HERE / name) != recorded:
            raise ValueError('prepared runner/helper changed: ' + name)
    if sorted(os.sched_getaffinity(0))[:2] != manifest['cpus']:
        raise ValueError('prepared CPU affinity changed')
    os.sched_setaffinity(0, manifest['cpus'])
    artifacts = {}
    cases = [f'{n}-{shape}' for n, shape in config['protocol']['cases']]
    expected_builds = {(case, name) for case in cases for name in ARM_OPTIONS}
    builds = json.loads((args.out / 'builds.json').read_text())
    if len(builds) != 30 or manifest.get('status') != 'Prepared' or not manifest.get('functional_valid') or not manifest.get('strategy_valid'):
        raise ValueError('preparation is not the complete valid thirty-build profile')
    for build in builds:
        key = build['case'], build['arm']
        if (key not in expected_builds or key in artifacts or build.get('stage') != 'Complete' or
                not build['valid'] or not build['functional_valid'] or not build['strategy_valid'] or
                build['native_controls'] != 5 or identity(build['executable']['path']) != build['executable']):
            raise ValueError('prepared artifact changed or failed')
        if build['arm'] != 'rust':
            expected_units = config['protocol']['quality_expected_units'][build['case']] if build['arm'].startswith('quality_') else 1
            if build['actual_units'] != expected_units or (build['arm'].startswith('quality_') and build['fallback'] is not None):
                raise ValueError('prepared strategy changed')
        artifacts[key] = build['executable']['path']
    if set(artifacts) != expected_builds or set(manifest['sources']) != set(cases) or set(manifest['expected']) != set(cases):
        raise ValueError('prepared case/arm/source/oracle grid is incomplete')
    oracles = json.loads((args.out / 'native-oracles.json').read_text())
    expected_oracles = {(case, arm, seed) for case, arm in expected_builds for seed in SEEDS}
    observed_oracles = set()
    for row in oracles:
        key = row['case'], row['arm'], row['seed']
        if key not in expected_oracles or key in observed_oracles:
            raise ValueError('native preflight grid is duplicated or incomplete')
        expected_hex = manifest['expected'][key[0]][str(key[2])].to_bytes(4, 'little').hex()
        process = row['process']
        if (not row['valid'] or row['expected_hex'] != expected_hex or row['stdout_hex'] != expected_hex or row['stderr_hex'] or
                process['command'] != [artifacts[key[0], key[1]]] or process['exit'] != 0 or process['timeout'] or
                process['launch_error'] is not None or Path(process['stdout_path']).read_bytes().hex() != expected_hex or
                Path(process['stderr_path']).read_bytes()):
            raise ValueError('native preflight contains an invalid raw outcome')
        observed_oracles.add(key)
    if len(oracles) != 150 or observed_oracles != expected_oracles:
        raise ValueError('native preflight is not the complete 150-observation grid')
    for sources in manifest['sources'].values():
        for source in sources.values():
            if identity(source['path']) != source:
                raise ValueError('prepared source changed')
    for case in cases:
        if manifest['expected'][case] != prior['expected'][case]:
            raise ValueError('prepared oracle differs from the frozen same-source oracle')
        for name in ['main.zil', 'main.rs']:
            actual, reference = manifest['sources'][case][name], prior['sources'][case][name]
            if (actual['sha256'], actual['bytes']) != (reference['sha256'], reference['bytes']):
                raise ValueError('prepared source differs from the frozen six-family suite')
    execution = {'started_at_utc': now(), 'command': sys.argv, 'manifest_sha256': marker['manifest_sha256'],
                 'completed': False, 'valid': False, 'observations': 0, 'failed_observations': 0,
                 'prebuilt_artifacts_only': True, 'planned': 900,
                 'lease': {'path': str(args.lease.resolve()), 'sha256': hashlib.sha256(lease_bytes).hexdigest(),
                           'bytes': len(lease_bytes)},
                 'lease_contents': lease, 'lease_raw_utf8': lease_bytes.decode('utf-8')}
    with (args.out / 'execution.json').open('x') as stream:
        stream.write(json.dumps(execution, indent=2) + '\n')
    args._owns_epoch = True
    names = list(ARM_OPTIONS)
    try:
        for n, shape in config['protocol']['cases']:
            case = f'{n}-{shape}'
            for block in range(6):
                order = names if block % 2 == 0 else list(reversed(names))
                for seed in SEEDS:
                    for position, name in enumerate(order):
                        args._active_operation = {'phase': 'formal_native', 'case': case, 'block': block,
                                                  'seed': seed, 'position': position, 'arm': name}
                        prefix = args.out / 'runtime' / case / f'{block}-{seed}-{position}-{name}'
                        row = native_control(artifacts[case, name], prefix, seed, manifest['expected'][case][str(seed)])
                        row.update(case=case, block=block, position=position, arm=name)
                        with (args.out / 'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(row) + '\n')
                        execution['observations'] += 1
                        execution['failed_observations'] += not row['valid']
                        save(args.out / 'execution.json', execution)
                        args._active_operation = None
                print('RUNTIME', case, block + 1, '/ 6', flush=True)
        execution['completed'] = execution['observations'] == 900
        execution['valid'] = execution['completed'] and execution['failed_observations'] == 0
    finally:
        execution['finished_at_utc'] = now()
        execution['interrupted_observation'] = args._active_operation
        execution['not_run_observations'] = 900 - execution['observations'] - bool(args._active_operation)
        save(args.out / 'execution.json', execution)
    if not execution['valid']:
        raise RuntimeError('formal observations failed or incomplete; all results retained')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    create = sub.add_parser('create-config')
    create.add_argument('--production-build', type=Path, required=True)
    create.add_argument('--rust-reference', type=Path, required=True)
    create.add_argument('--reference-prepared', type=Path, required=True)
    create.add_argument('--boundary-8192-branch', action='store_true')
    create.add_argument('--out', type=Path, required=True)
    for name in ['prepare', 'measure']:
        selected = sub.add_parser(name)
        selected.add_argument('--config', type=Path, required=True)
        selected.add_argument('--out', type=Path, required=True)
        selected.add_argument('--attempt-receipt', type=Path)
        if name == 'measure':
            selected.add_argument('--lease', type=Path, required=True)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args._owns_epoch = False
    args._active_operation = None
    if hasattr(args, 'config'):
        args.config = args.config.resolve()
    attempt_path = getattr(args, 'attempt_receipt', None) or args.out.with_name(args.out.name + '-' + args.operation + '-attempt.json')
    attempt_path = attempt_path.resolve()
    if attempt_path == args.out or args.out in attempt_path.parents:
        raise ValueError('attempt receipt must be outside the protected epoch')
    attempt_path.parent.mkdir(parents=True, exist_ok=True)
    attempt = {'status': 'Running', 'operation': args.operation, 'output': str(args.out),
               'command': sys.argv, 'started_at_utc': now(), 'epoch_mutation_started': False}
    with attempt_path.open('x') as stream:
        stream.write(json.dumps(attempt, indent=2) + '\n')
    previous_term = signal.getsignal(signal.SIGTERM)
    signal.signal(signal.SIGTERM, terminate_requested)
    try:
        {'create-config': create_config, 'prepare': prepare, 'measure': measure}[args.operation](args)
        attempt['status'] = 'Pass'
    except BaseException as error:
        interrupted = isinstance(error, (KeyboardInterrupt, TerminationRequested))
        attempt.update(status='Interrupted' if interrupted else 'Failed', error=repr(error),
                       active_operation=args._active_operation, epoch_mutation_started=args._owns_epoch)
        if args._owns_epoch and args.out.is_dir():
            with (args.out / 'failures.jsonl').open('a') as stream:
                stream.write(json.dumps({'time_utc': now(), 'command': sys.argv,
                    'error_type': type(error).__name__, 'error': str(error), 'traceback': traceback.format_exc()}) + '\n')
            if args.operation == 'prepare':
                manifest = json.loads((args.out / 'manifest.json').read_text())
                manifest.update(operation_outcome=attempt['status'], active_operation=args._active_operation,
                                failure=repr(error), finished_at_utc=now())
                if manifest['status'] == 'Preparing':
                    manifest['status'] = 'Interrupted' if interrupted else 'PreparationFailed'
                if 'configuration' in manifest:
                    planned = [(f'{n}-{shape}', arm['name']) for n, shape in manifest['protocol']['cases']
                               for arm in manifest['configuration']['arms']]
                    builds_path = args.out / 'builds.json'
                    observed = json.loads(builds_path.read_text()) if builds_path.exists() else []
                    attempted = {(build['case'], build['arm']) for build in observed}
                    if args._active_operation and 'arm' in args._active_operation:
                        attempted.add((args._active_operation['case'], args._active_operation['arm']))
                    manifest['not_run_builds'] = [{'case': case, 'arm': arm, 'status': 'NotRun'}
                                                  for case, arm in planned if (case, arm) not in attempted]
                    manifest['attempted_builds'] = len(attempted)
                save(args.out / 'manifest.json', manifest)
        raise
    finally:
        signal.signal(signal.SIGTERM, previous_term)
        attempt.update(finished_at_utc=now(), epoch_mutation_started=args._owns_epoch)
        save(attempt_path, attempt)


if __name__ == '__main__':
    main()

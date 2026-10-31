#!/usr/bin/env python3
"""Prepare fixed toolchain artifacts, then measure opaque-input native runtime only."""
import argparse
from contextlib import suppress
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import traceback

from stage6_merge_runtime import CASES, SEEDS, generate

HERE = Path(__file__).resolve().parent
ARM_OPTIONS = {
    'baseline_off': ['--function-merging', 'off'],
    'baseline_off_closed': ['--function-merging', 'off', '--closed-executable'],
    'original_after': ['--function-merging', 'after'],
    'original_after_closed': ['--function-merging', 'after', '--closed-executable'],
    'entry_after': ['--function-merging', 'after'],
    'entry_after_closed': ['--function-merging', 'after', '--closed-executable'],
    'entry_incremental_after': ['--function-merging', 'after', '--incremental-o2'],
    'entry_incremental_after_closed': ['--function-merging', 'after', '--incremental-o2', '--closed-executable'],
    'rust': [],
}
ISOLATED_ENV = ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT', 'LD_DEBUG',
                'LD_DEBUG_OUTPUT', 'ZIL_CLANG', 'ZIL_LLVM_LIB_DIR']


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def identity(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path), 'bytes': path.stat().st_size}


def checked_identity(spec, base):
    path = Path(spec['path'])
    if not path.is_absolute():
        path = base / path
    actual = identity(path)
    if actual['sha256'] != spec['sha256']:
        raise ValueError('identity mismatch: ' + str(path))
    return actual


def clean_environment():
    result = os.environ.copy()
    for name in ISOLATED_ENV:
        result.pop(name, None)
    return result


def arm_environment(arm):
    result = clean_environment()
    result.update(arm['env'])
    return result


def command_run(command, prefix, environment, timeout, stdin=None):
    """Persist launch, timeout and partial-output outcomes, including whole-group kill."""
    prefix.parent.mkdir(parents=True, exist_ok=True)
    record = {'command': [str(x) for x in command], 'started_at_utc': now(),
              'timeout': False, 'launch_error': None, 'exit': None,
              'environment_overrides': {k: environment[k] for k in ISOLATED_ENV if k in environment}}
    out_path, err_path = Path(str(prefix) + '.stdout'), Path(str(prefix) + '.stderr')
    child = None
    prior_handler = signal.getsignal(signal.SIGALRM)

    def expired(_signal, _frame):
        raise TimeoutError('process timeout')

    with out_path.open('wb') as out, err_path.open('wb') as err:
        started = time.perf_counter_ns()
        try:
            child = subprocess.Popen(record['command'], stdout=out, stderr=err,
                                     stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                     env=environment, start_new_session=True)
            record['pid'] = child.pid
            signal.signal(signal.SIGALRM, expired)
            signal.alarm(timeout)
            if stdin is not None:
                try:
                    child.stdin.write(stdin)
                    child.stdin.close()
                except BrokenPipeError:
                    pass
            try:
                _, status, usage = os.wait4(child.pid, 0)
            except TimeoutError:
                record['timeout'] = True
                with suppress(ProcessLookupError):
                    os.killpg(child.pid, signal.SIGKILL)
                _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            record.update(exit=child.returncode, cpu_seconds=usage.ru_utime + usage.ru_stime,
                          max_rss_kib=usage.ru_maxrss)
        except OSError as error:
            record['launch_error'] = str(error)
            if child is not None and child.returncode is None:
                with suppress(ProcessLookupError):
                    os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        except BaseException as error:
            record['interruption'] = repr(error)
            if child is not None and child.returncode is None:
                with suppress(ProcessLookupError):
                    os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            raise
        finally:
            signal.alarm(0)
            signal.signal(signal.SIGALRM, prior_handler)
            record.update(elapsed_ns=time.perf_counter_ns() - started, finished_at_utc=now(),
                          stdout_path=str(out_path), stderr_path=str(err_path))
            save(Path(str(prefix) + '.process.json'), record)
    return record


def load_config(path, smoke):
    config = json.loads(path.read_text())
    if config.get('schema') != 1:
        raise ValueError('configuration schema must be 1')
    if not smoke:
        variant = config['candidate_variant']
        original_variant = config.get('original_variant', 'original')
        namespace = config.get('pipeline_identity_namespace', 'research-llvm-jt')
        if not isinstance(variant, str) or not re.fullmatch(r'[a-z][a-z0-9_+.-]*', variant) or variant in ['original', 'baseline', 'stock']:
            raise ValueError('candidate_variant must explicitly name the distinct candidate toolchain')
        if (not isinstance(original_variant, str) or
                not re.fullmatch(r'[a-z][a-z0-9_+.-]*', original_variant) or
                original_variant in [variant, 'baseline', 'stock']):
            raise ValueError('original_variant must explicitly name the distinct control toolchain')
        if namespace not in ['research-llvm-jt', 'research-llvm-stage6']:
            raise ValueError('unsupported pipeline identity namespace')
        for field in ['baseline_revision', 'candidate_revision']:
            if not re.fullmatch(r'[0-9a-f]{40}', config[field]):
                raise ValueError('formal source revisions must be full Git commit IDs')
    arms = []
    for supplied in config['arms']:
        arm = dict(supplied)
        name = arm['name']
        if name not in ARM_OPTIONS or arm['options'] != ARM_OPTIONS[name]:
            raise ValueError('unknown arm or unexpected options: ' + name)
        if arm['language'] != ('rust' if name == 'rust' else 'zil'):
            raise ValueError('wrong language for arm: ' + name)
        arm['compiler'] = checked_identity(arm['compiler'], path.parent)
        overrides = arm.get('env', {})
        if not isinstance(overrides, dict) or any(k not in ['LD_LIBRARY_PATH', 'ZIL_CLANG']
                or not isinstance(v, str) for k, v in overrides.items()):
            raise ValueError('arm env accepts only explicit LD_LIBRARY_PATH/ZIL_CLANG strings')
        arm['env'] = overrides
        if name == 'rust':
            if overrides or supplied.get('llvm') is not None:
                raise ValueError('Rust must not inherit a ZIL LLVM environment')
        else:
            arm['source_receipt'] = checked_identity(arm['source_receipt'], path.parent)
            llvm = dict(arm['llvm'])
            llvm['library'] = checked_identity(llvm['library'], path.parent)
            llvm['receipt'] = checked_identity(llvm['receipt'], path.parent)
            receipt_path = Path(llvm['receipt']['path'])
            receipt = json.loads(receipt_path.read_text())
            if receipt.get('status') != 'Pass':
                raise ValueError('toolchain receipt did not pass: ' + str(receipt_path))
            recorded = receipt['library']
            recorded_path = Path(recorded['path'])
            if not recorded_path.is_absolute():
                recorded_path = receipt_path.parent / recorded_path
            if (recorded_path.resolve(strict=True) != Path(llvm['library']['path'])
                    or recorded['sha256'] != llvm['library']['sha256']):
                raise ValueError('DSO does not match toolchain receipt: ' + name)
            if not overrides.get('LD_LIBRARY_PATH'):
                raise ValueError('ZIL arm requires explicit LD_LIBRARY_PATH: ' + name)
            llvm['build_id'] = recorded.get('build_id')
            llvm['version'] = recorded.get('version')
            llvm['receipt_arm'] = receipt.get('arm')
            if not smoke:
                expected_arm = original_variant if name.startswith('original_') else variant if name.startswith('entry_') else None
                if arm.get('expected_toolchain_arm') != expected_arm or llvm['receipt_arm'] != expected_arm:
                    raise ValueError('toolchain receipt belongs to another experimental arm: ' + name)
                provenance = json.loads(Path(arm['source_receipt']['path']).read_text())
                if expected_arm is None:
                    if (provenance['commit'] != config['baseline_revision'] or
                            Path(provenance['binary']).resolve(strict=True) != Path(arm['compiler']['path']) or
                            provenance['sha256'] != arm['compiler']['sha256']):
                        raise ValueError('baseline compiler does not match its build/source receipt')
                    arm['source_revision'] = provenance['commit']
                else:
                    tag = '|' + namespace + '=' + expected_arm + '|dso-sha256=' + llvm['library']['sha256']
                    if (provenance['revision'] != config['candidate_revision'] or
                            provenance['arm'] != expected_arm or provenance['production_mutated'] is not False or
                            provenance['toolchain_receipt']['sha256'] != llvm['receipt']['sha256'] or
                            Path(provenance['toolchain_receipt']['path']).resolve(strict=True) != Path(llvm['receipt']['path']) or
                            provenance['library']['sha256'] != llvm['library']['sha256'] or
                            Path(provenance['library']['path']).resolve(strict=True) != Path(llvm['library']['path']) or
                            provenance['pipeline_identity_tag'] != tag):
                        raise ValueError('source snapshot is not bound to this revision/toolchain/tag: ' + name)
                    arm.update(source_revision=provenance['revision'], expected_pipeline_tag=tag,
                               source_originals_sha256=provenance['original_files_manifest_sha256'])
                arm['toolchain_variant'] = expected_arm or 'stock'
            arm['llvm'] = llvm
        arms.append(arm)
    names = [arm['name'] for arm in arms]
    if not arms or len(set(names)) != len(names):
        raise ValueError('arms must be nonempty and unique')
    if not smoke and names != list(ARM_OPTIONS):
        raise ValueError('formal arm order must be the documented nine-arm order')
    if not smoke:
        groups = {}
        for prefix in ['baseline_', 'original_', 'entry_']:
            members = [arm for arm in arms if arm['name'].startswith(prefix)]
            first = members[0]
            for arm in members[1:]:
                if any(arm[field] != first[field] for field in ['compiler', 'source_receipt', 'env', 'llvm']):
                    raise ValueError('compiler/provenance/environment changes within role: ' + prefix)
            groups[prefix] = first
        if groups['original_']['llvm']['library']['sha256'] == groups['entry_']['llvm']['library']['sha256']:
            raise ValueError('original and entry toolchains must be distinct DSOs')
        if groups['original_']['source_originals_sha256'] != groups['entry_']['source_originals_sha256']:
            raise ValueError('original and candidate snapshots do not share the same original source inventory')
    tools = {name: checked_identity(config['tools'][name], path.parent)
             for name in ['clang', 'nm', 'size']}
    return arms, tools


def loader_audit(prefix, build, arm):
    result = {'valid': False, 'expected_library': arm['llvm']['library'],
              'scope': 'LD_DEBUG=libs init records for the actual compiler PID; child process logs are retained separately'}
    if 'pid' not in build:
        result['error'] = 'compiler did not start'
        return result
    path = Path(str(prefix) + '.' + str(build['pid']))
    result['log'] = str(path)
    try:
        text = path.read_text(errors='replace')
        matches = re.findall(r'calling init:\s+([^\n]*libLLVM[^\n]*)', text)
        libraries = sorted({str(Path(value.strip()).resolve(strict=True)) for value in matches})
        result.update(log_sha256=sha(path), loaded_libraries=libraries)
        result['valid'] = libraries == [arm['llvm']['library']['path']]
        if result['valid']:
            result['loaded_library_sha256'] = sha(libraries[0])
            result['valid'] = result['loaded_library_sha256'] == arm['llvm']['library']['sha256']
    except OSError as error:
        result['error'] = str(error)
    return result


def executable_audits(exe, directory, tools):
    result = {'valid': True, 'scope': 'nm global definitions and dynamic definitions are distinct inventories; neither invents an export contract'}
    commands = {
        'nm-global': [tools['nm']['path'], '--extern-only', '--defined-only', '--format=posix', str(exe)],
        'nm-dynamic': [tools['nm']['path'], '-D', '--defined-only', '--format=posix', str(exe)],
        'size': [tools['size']['path'], '--format=berkeley', str(exe)],
    }
    for name, command in commands.items():
        outcome = command_run(command, directory / name, clean_environment(), 30)
        stdout = Path(outcome['stdout_path']).read_bytes()
        stderr = Path(outcome['stderr_path']).read_bytes()
        valid = outcome['exit'] == 0 and not outcome['timeout'] and not stderr
        record = {'process': outcome, 'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
                  'stderr_hex': stderr.hex(), 'valid': valid}
        if valid and name.startswith('nm-'):
            record['symbols'] = [line.split()[0] for line in stdout.decode().splitlines() if line.split()]
            record['zil_function_count'] = sum(x.startswith('zil.fn.') for x in record['symbols'])
        if valid and name == 'size':
            try:
                record['text_bytes'], record['data_bytes'], record['bss_bytes'] = map(int, stdout.decode().splitlines()[-1].split()[:3])
            except (ValueError, IndexError) as error:
                record.update(valid=False, error=str(error))
        result[name] = record
        result['valid'] &= record['valid']
    return result


def prepare(args):
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = {'schema': 1, 'started_at_utc': now(), 'status': 'Preparing',
                'functional_smoke_only': args.functional_smoke, 'scope': __doc__,
                'compile_timing_boundary': 'One diagnostic build per case/arm, including loader audit instrumentation; not a repeated compilation benchmark',
                'runtime_timing_boundary': 'Fresh native process, one opaque input byte, complete output; only prebuilt artifacts; compiler/loader audits and oracle outside timing',
                'blocks': args.blocks, 'rounds': args.rounds, 'seeds': SEEDS,
                'cpus': sorted(os.sched_getaffinity(0))[:2], 'sources': {}, 'expected': {},
                'cases': args.case or [f'{n}-{shape}' for n, shape in CASES],
                'arm_order': 'Forward/reverse in alternating blocks; each pair has balanced relative order, not all nine absolute positions',
                'config_requested': str(args.config)}
    save(args.out / 'manifest.json', manifest)
    arms, tools = load_config(args.config, args.functional_smoke)
    manifest.update(config=identity(args.config), arms=arms, tools=tools,
                    candidate_variant=json.loads(args.config.read_text()).get('candidate_variant', 'functional-smoke'),
                    original_variant=json.loads(args.config.read_text()).get('original_variant', 'original'),
                    pipeline_identity_namespace=json.loads(args.config.read_text()).get('pipeline_identity_namespace', 'research-llvm-jt'),
                    isolated_environment_keys=ISOLATED_ENV,
                    inherited_path=os.environ.get('PATH', ''),
                    scripts={name: identity(HERE / name) for name in [Path(__file__).name, 'stage6_merge_runtime.py', 'stage6_bench.py']},
                    planned_builds=len(manifest['cases']) * len(arms),
                    planned_runtime_observations=len(manifest['cases']) * len(arms) * args.blocks * len(SEEDS))
    save(args.out / 'manifest.json', manifest)
    os.sched_setaffinity(0, manifest['cpus'])
    scripts = args.out / 'scripts'
    scripts.mkdir()
    for name in manifest['scripts']:
        shutil.copy2(HERE / name, scripts / name)
    shutil.copy2(args.config, args.out / 'configuration.json')
    builds = []
    save(args.out / 'builds.json', builds)
    for n, shape in CASES:
        case = f'{n}-{shape}'
        if case not in manifest['cases']:
            continue
        directory = args.out / case
        directory.mkdir()
        generate(directory, n, shape, args.rounds)
        manifest['sources'][case] = {p.name: identity(p) for p in [directory / 'main.zil', directory / 'main.rs']}
        values = [sum(((x + 64 + i % 97) % 1009 if shape == 'body' else
                       (x * 3 + i % 97) % 1009 if shape != 'branch' or x % 3 == 0 else
                       (x + i % 31) % 1009) for i in range(n)) for x in range(1009)]
        expected = {}
        for seed in SEEDS:
            total, x = 0, seed
            for _ in range(args.rounds):
                total = (total + values[x]) % 1000003
                x = (x + 17) % 1009
            expected[str(seed)] = total
        manifest['expected'][case] = expected
        save(args.out / 'manifest.json', manifest)
        for arm in arms:
            name = arm['name']
            work = directory / name
            work.mkdir()
            exe = work / 'program'
            environment = arm_environment(arm)
            command = [arm['compiler']['path']]
            if arm['language'] == 'rust':
                command += [str(directory / 'main.rs'), '--edition=2024', '-A', 'warnings',
                            '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                            '-C', 'codegen-units=8', '-o', str(exe)]
            else:
                command += ['build', str(directory / 'main.zil'), '--backend', 'llvm-api',
                            '--opt', 'O2', '--codegen-units', '8', '--codegen-jobs', '2',
                            '--clang', tools['clang']['path'], '--json', '-o', str(exe), *arm['options']]
                environment.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(work / 'loader'))
            outcome = command_run(command, work / 'build', environment, args.compile_timeout)
            build = {'case': case, 'arm': name, 'process': outcome,
                     'valid': outcome['exit'] == 0 and not outcome['timeout'] and exe.is_file()}
            if arm['language'] == 'zil':
                build['loader_audit'] = loader_audit(work / 'loader', outcome, arm)
                build['valid'] &= build['loader_audit']['valid']
                try:
                    report = json.loads(Path(outcome['stdout_path']).read_text())
                    build.update(verification_runs=report['verification_runs'], status=report['status'],
                                 native_codegen=report['native_codegen'])
                    build['valid'] &= report['status'] == 'Success' and report['verification_runs'] == 1
                    codegen = report['native_codegen']
                    units = codegen['actual_units']
                    build['actual_api_controls'] = {
                        'backend': codegen['backend'] == 'llvm-api',
                        'has_actual_units': isinstance(units, int) and units > 0,
                        'llvm_verified_before_after': bool(codegen['object_reports']) and
                            all(item['verify_before_after'] for item in codegen['object_reports']),
                        'incremental_arm_has_multiple_units': '--incremental-o2' not in arm['options'] or units > 1,
                        'actual_pipeline_identity_tag': 'expected_pipeline_tag' not in arm or
                            all(item['pipeline'].endswith(arm['expected_pipeline_tag']) for item in codegen['object_reports']),
                    }
                    build['valid'] &= all(build['actual_api_controls'].values())
                except (ValueError, KeyError, TypeError) as error:
                    build.update(valid=False, report_error=str(error))
            if build['valid']:
                build['executable'] = identity(exe)
                build['audits'] = executable_audits(exe, work, tools)
                build['valid'] &= build['audits']['valid']
            builds.append(build)
            save(work / 'build-result.json', build)
            save(args.out / 'builds.json', builds)
            print('BUILD', case, name, 'valid=' + str(build['valid']), flush=True)
    ready = len(builds) == manifest['planned_builds'] and all(build['valid'] for build in builds)
    manifest.update(status='Prepared' if ready else 'PreparationFailed', finished_at_utc=now(),
                    failed_builds=sum(not build['valid'] for build in builds))
    save(args.out / 'manifest.json', manifest)
    save(args.out / 'prepared.json', {'ready': ready, 'manifest_sha256': sha(args.out / 'manifest.json'),
                                    'builds_sha256': sha(args.out / 'builds.json')})
    if not ready:
        raise RuntimeError('preparation contains failed builds; all outcomes retained')


def run_prepared(args):
    if (args.out / 'execution.json').exists() or (args.out / 'rows.jsonl').exists():
        raise ValueError('formal output already exists; never overwrite or silently resume')
    manifest = json.loads((args.out / 'manifest.json').read_text())
    marker = json.loads((args.out / 'prepared.json').read_text())
    if marker != {'ready': True, 'manifest_sha256': sha(args.out / 'manifest.json'),
                  'builds_sha256': sha(args.out / 'builds.json')}:
        raise ValueError('preparation incomplete or changed')
    for field, value in [('blocks', args.blocks), ('rounds', args.rounds),
                         ('functional_smoke_only', args.functional_smoke),
                         ('cases', args.case or [f'{n}-{shape}' for n, shape in CASES])]:
        if manifest[field] != value:
            raise ValueError('prepared execution configuration changed: ' + field)
    arms, tools = load_config(args.config, args.functional_smoke)
    if arms != manifest['arms'] or tools != manifest['tools'] or identity(args.config) != manifest['config']:
        raise ValueError('arm/tool configuration changed')
    for name, recorded in manifest['scripts'].items():
        if identity(HERE / name) != recorded:
            raise ValueError('runner/helper changed: ' + name)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    if cpus != manifest['cpus']:
        raise ValueError('prepared CPU affinity changed')
    os.sched_setaffinity(0, cpus)
    builds = json.loads((args.out / 'builds.json').read_text())
    artifacts = {}
    for build in builds:
        if not build['valid'] or identity(build['executable']['path']) != build['executable']:
            raise ValueError('prepared executable changed or invalid')
        artifacts[build['case'], build['arm']] = build['executable']['path']
    for sources in manifest['sources'].values():
        for recorded in sources.values():
            if identity(recorded['path']) != recorded:
                raise ValueError('prepared source changed')
    execution = {'started_at_utc': now(), 'command': sys.argv, 'manifest_sha256': sha(args.out / 'manifest.json'),
                 'completed': False, 'valid': False, 'observations': 0, 'failed_observations': 0,
                 'prebuilt_artifacts_only': True}
    save(args.out / 'execution.json', execution)
    names = [arm['name'] for arm in arms]
    try:
        for case in manifest['cases']:
            for block in range(args.blocks):
                order = names if block % 2 == 0 else list(reversed(names))
                for seed in SEEDS:
                    expected = manifest['expected'][case][str(seed)]
                    for position, name in enumerate(order):
                        prefix = args.out / 'runtime' / case / f'{block}-{seed}-{position}-{name}'
                        outcome = command_run([artifacts[case, name]], prefix, clean_environment(),
                                              args.runtime_timeout, stdin=bytes([seed]))
                        stdout = Path(outcome['stdout_path']).read_bytes()
                        stderr = Path(outcome['stderr_path']).read_bytes()
                        row = {'case': case, 'block': block, 'seed': seed, 'position': position,
                               'arm': name, 'expected': expected, 'expected_hex': expected.to_bytes(4, 'little').hex(),
                               'stdout_hex': stdout.hex(), 'stderr_hex': stderr.hex(), 'process': outcome,
                               'elapsed_ns': outcome['elapsed_ns'],
                               'valid': outcome['exit'] == 0 and not outcome['timeout'] and
                                        stdout == expected.to_bytes(4, 'little') and not stderr}
                        with (args.out / 'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(row) + '\n')
                        execution['observations'] += 1
                        execution['failed_observations'] += not row['valid']
                        save(args.out / 'execution.json', execution)
                print('RUNTIME', case, block + 1, '/', args.blocks, flush=True)
        execution['completed'] = execution['observations'] == manifest['planned_runtime_observations']
        execution['valid'] = execution['completed'] and execution['failed_observations'] == 0
    finally:
        execution['finished_at_utc'] = now()
        save(args.out / 'execution.json', execution)
    print(json.dumps(execution), flush=True)
    if not execution['valid']:
        raise RuntimeError('runtime observations incomplete or failed; every outcome retained')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--prepare-only', action='store_true')
    mode.add_argument('--run-prepared', action='store_true')
    parser.add_argument('--blocks', type=int, default=6)
    parser.add_argument('--rounds', type=int, default=20000)
    parser.add_argument('--case', action='append', choices=[f'{n}-{shape}' for n, shape in CASES])
    parser.add_argument('--functional-smoke', action='store_true')
    parser.add_argument('--compile-timeout', type=int, default=240)
    parser.add_argument('--runtime-timeout', type=int, default=60)
    args = parser.parse_args()
    args.config, args.out = args.config.resolve(), args.out.resolve()
    if min(args.blocks, args.rounds, args.compile_timeout, args.runtime_timeout) < 1:
        parser.error('counts and timeouts must be positive')
    if args.case and len(set(args.case)) != len(args.case):
        parser.error('duplicate case')
    if not args.functional_smoke and (args.blocks != 6 or args.rounds != 20000 or args.case):
        parser.error('formal protocol fixes all six cases, six blocks, 20000 rounds')
    try:
        prepare(args) if args.prepare_only else run_prepared(args)
    except BaseException as error:
        if args.out.is_dir():
            with (args.out / 'failures.jsonl').open('a') as stream:
                stream.write(json.dumps({'time_utc': now(), 'command': sys.argv,
                    'error_type': type(error).__name__, 'error': str(error),
                    'traceback': traceback.format_exc()}) + '\n')
        raise


if __name__ == '__main__':
    main()

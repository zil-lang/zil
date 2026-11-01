#!/usr/bin/env python3
"""One current-source actual-CGU request; validate native output before atomic per-key cache publication."""
import argparse
import hashlib
import json
import os
import signal
import subprocess
import time
import traceback
from pathlib import Path

from cache_bundle import publish_bundle


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path):
    # Multicall argv0 matters: ld.lld must not become generic lld.
    path = path.absolute()
    stat = path.stat()
    return {'path': str(path), 'resolved_path': str(path.resolve()), 'bytes': stat.st_size,
            'mtime_ns': stat.st_mtime_ns, 'sha256': sha(path)}


def run_owned(command, timeout, env=None):
    """Run only our child process group; preserve timeout output and kill descendants."""
    process = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
    return process.returncode, timed_out, stdout, stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['zil', 'clang', 'opt', 'plugin', 'lld', 'pipeline-manifest', 'snapshot-receipt', 'source', 'out', 'cache']:
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--arm', choices=['builtin', 'original', 'entry'], required=True)
    parser.add_argument('--closed', action='store_true')
    parser.add_argument('--incremental-o2', action='store_true')
    parser.add_argument('--verify-analyses', action='store_true')
    parser.add_argument('--verify-each', action='store_true')
    parser.add_argument('--diagnostics', action='store_true')
    parser.add_argument('--native-input', type=Path)
    parser.add_argument('--expected-hex')
    parser.add_argument('--timeout', type=float, default=180)
    parser.add_argument('--native-timeout', type=float, default=60)
    parser.add_argument('--units', type=int, default=8)
    parser.add_argument('--jobs', type=int, default=1)
    args = parser.parse_args()
    args.out, args.cache = args.out.resolve(), args.cache.resolve()
    start_service = time.perf_counter_ns()
    native_elapsed = 0
    phase = 'setup'
    result = {'exit': None, 'timeout': False, 'driver_elapsed_ns': 0, 'status': None,
              'verification_runs': None, 'native_codegen': None, 'current_preflight_units': 0,
              'cache_hits': 0, 'cache_misses': 0, 'corrupt_rebuilt': 0,
              'cache_published': False, 'published_entries': [], 'cache_publish_failures': [],
              'publication_scope': 'Atomic per key after successful native oracle. A successful build may have explicit nonfatal cache-write failures; multi-key publication is not all-or-nothing.',
              'errors': [], 'valid': False}
    manifest = {'scope': __doc__, 'command': None}
    try:
        args.out.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        # Refuse to overwrite an existing request or invent a receipt in an
        # unwritable destination; the caller still receives structured output.
        result['errors'].append({'phase': phase, 'kind': type(error).__name__, 'message': str(error)})
        result['service_elapsed_ns'] = time.perf_counter_ns()-start_service
        print(json.dumps(result, indent=2))
        raise SystemExit(1)
    try:
        if args.timeout <= 0 or args.native_timeout <= 0:
            raise ValueError('timeouts must be positive')
        args.cache.mkdir(parents=True, exist_ok=True)
        wrapper = Path(__file__).with_name('clang_wrapper.py').resolve()
        tools = {name: identity(getattr(args, name)) for name in ['zil', 'clang', 'opt', 'plugin', 'lld']}
        pipeline = json.loads(args.pipeline_manifest.read_text())
        snapshot = json.loads(args.snapshot_receipt.read_text())
        if not isinstance(pipeline, dict) or not isinstance(snapshot, dict):
            raise ValueError('pipeline and snapshot receipts must be JSON objects')
        if pipeline['opt']['sha256'] != tools['opt']['sha256'] or pipeline['plugin']['sha256'] != tools['plugin']['sha256']:
            raise ValueError('pipeline tool identity changed')
        if snapshot['production_mutated'] is not False:
            raise ValueError('research snapshot boundary missing')
        (args.out/'snapshot-receipt.json').write_bytes(args.snapshot_receipt.read_bytes())
        config = {'arm': args.arm, 'pipeline': pipeline['pipelines'][args.arm], 'target_flags': pipeline['target_flags'],
                  'request': str(args.out), 'cache': str(args.cache), 'tools': tools,
                  'verify_analyses': args.verify_analyses, 'verify_each': args.verify_each,
                  'diagnostics': args.diagnostics, 'snapshot_sha256': sha(args.snapshot_receipt),
                  'pipeline_manifest_sha256': sha(args.pipeline_manifest)}
        config_path = args.out/'config.json'
        config_path.write_text(json.dumps(config, indent=2)+'\n')
        env = os.environ.copy()
        env.update(ZIL_JT_CONFIG=str(config_path), ZIL_THIN_CAPTURE=str(args.out/'units'), ZIL_THIN_IMPORTS_OFF='1')
        command = [str(args.zil.resolve()), 'build', str(args.source.resolve()), '--backend', 'clang',
                   '--clang', str(wrapper), '--opt', 'O2', '--function-merging', 'after', '--codegen-units',
                   str(args.units), '--codegen-jobs', str(args.jobs), '--json', '-o', str(args.out/'program')]
        if args.closed:
            command += ['--closed-executable']
        if args.incremental_o2:
            command += ['--incremental-o2']
        manifest.update(command=command, tools=tools, wrapper=identity(wrapper), runner=identity(Path(__file__)),
            cache_protocol=identity(Path(__file__).with_name('cache_bundle.py')),
            snapshot_sha256=sha(args.snapshot_receipt), pipeline_manifest_sha256=sha(args.pipeline_manifest),
            timeout_seconds=args.timeout, native_timeout_seconds=args.native_timeout,
            source=identity(args.source), native_input_sha256=sha(args.native_input) if args.native_input else None,
            expected_hex=args.expected_hex, manual_imports=False,
            timing_scope='Full service includes setup/audit/cache publication, subtracts actual native process wall. Functional diagnostics unless separately authorized exclusive window.')
        (args.out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
        phase = 'driver'
        start = time.perf_counter_ns()
        try:
            exit_code, timed_out, stdout, stderr = run_owned(command, args.timeout, env)
            result.update(exit=exit_code, timeout=timed_out)
            (args.out/'driver.stdout').write_bytes(stdout)
            (args.out/'driver.stderr').write_bytes(stderr)
        finally:
            result['driver_elapsed_ns'] = time.perf_counter_ns()-start
        phase = 'driver-report'
        report = json.loads(stdout)
        if not isinstance(report, dict):
            raise ValueError('driver JSON report is not an object')
        result.update(status=report.get('status'), verification_runs=report.get('verification_runs'), native_codegen=report.get('native_codegen'))
        if timed_out or exit_code != 0 or report.get('status') != 'Success' or report.get('verification_runs') != 1:
            result['errors'].append({'phase': 'driver', 'kind': 'RejectedOrFailed', 'message': 'Current driver did not produce a verified successful build'})
        else:
            phase = 'current-preflight'
            preflight = json.loads((args.out/'units/preflight.json').read_text())
            if not isinstance(preflight, list) or not preflight or not all(isinstance(x, dict) and x.get('machine_preflight_current') is True for x in preflight):
                raise ValueError('current machine preflight missing')
            inputs = [json.loads(path.read_text()) for path in args.out.glob('unit-*.ll.input.json')]
            if len(inputs) != len(preflight) or not all(isinstance(x, dict) for x in inputs):
                raise ValueError('actual CGU receipts do not match current preflight')
            result['current_preflight_units'] = len(preflight)
            result['cache_hits'] = sum(x['cache_hit'] for x in inputs)
            result['cache_misses'] = sum(not x['cache_hit'] for x in inputs)
            result['corrupt_rebuilt'] = sum(x['corrupt_rebuilt'] for x in inputs)
            phase = 'native-oracle'
            start = time.perf_counter_ns()
            try:
                native_exit, native_timeout, native_stdout, native_stderr = run_owned([str(args.out/'program')], args.native_timeout,
                    None) if args.native_input is None else run_native_input(args.out/'program', args.native_input.read_bytes(), args.native_timeout)
            finally:
                native_elapsed = time.perf_counter_ns()-start
            (args.out/'native.stdout').write_bytes(native_stdout)
            (args.out/'native.stderr').write_bytes(native_stderr)
            result['native'] = {'exit': native_exit, 'timeout': native_timeout, 'stdout_hex': native_stdout.hex(),
                'stderr_hex': native_stderr.hex(), 'diagnostic_wall_ns': native_elapsed, 'executable': identity(args.out/'program')}
            result['valid'] = not native_timeout and native_exit == 0 and not native_stderr and (args.expected_hex is None or native_stdout.hex() == args.expected_hex)
            if not result['valid']:
                result['errors'].append({'phase': phase, 'kind': 'NativeOracleFailed', 'message': 'Native exit/output/timeout contract failed; no pending entry published'})
            else:
                phase = 'cache-publication'
                # Successful native oracle is the final gate. Each bundle is
                # atomic; an IO failure preserves old data and is reported as
                # an explicit cache-write failure, not a semantic build failure.
                pending_root = args.out/'pending'
                pending_entries = list(pending_root.iterdir()) if pending_root.exists() else []
                for pending in pending_entries:
                    try:
                        published = publish_bundle(pending, args.cache/(pending.name+'.zip'))
                        result['published_entries'].append({'action_key': pending.name, **published})
                    except Exception as error:
                        result['cache_publish_failures'].append({'action_key': pending.name,
                            'kind': type(error).__name__, 'message': str(error), 'traceback': traceback.format_exc()})
                result['cache_published'] = not result['cache_publish_failures']
    except Exception as error:
        result['valid'] = False
        result['errors'].append({'phase': phase, 'kind': type(error).__name__, 'message': str(error), 'traceback': traceback.format_exc()})
    finally:
        result['service_elapsed_ns'] = time.perf_counter_ns()-start_service-native_elapsed
        result['failure_phase'] = phase if not result['valid'] else None
        (args.out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
        (args.out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['valid'] else 1)


def run_native_input(program, data, timeout):
    process = subprocess.Popen([str(program)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = process.communicate(data, timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
    return process.returncode, timed_out, stdout, stderr


if __name__ == '__main__':
    main()

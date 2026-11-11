#!/usr/bin/env python3
"""Frozen A/B/R full-native runtime quality gate. No work without --execute."""
import argparse
import collections
import datetime
import json
import math
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import time
import traceback
from fixture import sha
from screen import artifact_identity, frequency, validate_clang_pin, validate_zil, zil_command

ACTIVE = {'phase': 'arguments', 'compilations_completed': 0, 'native_rows_written': 0}


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def measured(command, log, timeout, data=None):
    log.mkdir(parents=True, exist_ok=False)
    invocation_start = time.perf_counter_ns()
    result = {'command': command, 'exit': None, 'timeout': False, 'exception': None,
              'cpu_ms': None, 'rss_kib': None, 'wall_ms': None,
              'cleanup': [], 'reaped': False,
              'context': {k: ACTIVE.get(k) for k in ['phase', 'case', 'arm', 'round', 'position']}}
    previous = signal.getsignal(signal.SIGALRM)
    process, usage, status, raised = None, None, None, None
    start, end, armed = None, None, False
    def expired(*_):
        raise TimeoutError('frozen process deadline')
    def terminate_and_reap(reason):
        nonlocal status, usage
        if process is None or result['reaped']:
            return
        action = {'reason': reason, 'owned_process_group': process.pid}
        try:
            os.killpg(process.pid, signal.SIGKILL)
            action['kill'] = 'sent-SIGKILL'
        except ProcessLookupError:
            action['kill'] = 'already-gone'
        except OSError as error:
            action['kill_error'] = repr(error)
        try:
            _, status, usage = os.wait4(process.pid, 0)
            result['reaped'] = True
            action['wait4'] = 'reaped'
        except ChildProcessError as error:
            action['wait4_error'] = repr(error)
        result['cleanup'].append(action)
    try:
        if data is not None:
            (log / 'stdin').write_bytes(data)
        with (log / 'stdout').open('wb') as stdout, (log / 'stderr').open('wb') as stderr:
            start = time.perf_counter_ns()
            try:
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr,
                                           stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                           start_new_session=True)
            except OSError as error:
                result['exception'] = {'type': type(error).__name__, 'message': str(error), 'stage': 'launch'}
            if process is not None:
                signal.signal(signal.SIGALRM, expired)
                signal.setitimer(signal.ITIMER_REAL, timeout)
                armed = True
                try:
                    if data is not None:
                        try:
                            process.stdin.write(data)
                            process.stdin.close()
                        except BrokenPipeError:
                            result['stdin_broken_pipe'] = True
                    _, status, usage = os.wait4(process.pid, 0)
                    result['reaped'] = True
                except TimeoutError as error:
                    result['timeout'] = True
                    result['exception'] = {'type': type(error).__name__, 'message': str(error), 'stage': 'deadline'}
                    terminate_and_reap('timeout')
                except BaseException as error:
                    raised = error
                    result['exception'] = {'type': type(error).__name__, 'message': str(error), 'stage': 'wait-or-input'}
                    terminate_and_reap('exception-or-cancellation')
                finally:
                    end = time.perf_counter_ns()
    except BaseException as error:
        raised = error
        result['exception'] = {'type': type(error).__name__, 'message': str(error), 'stage': 'process-wrapper'}
        terminate_and_reap('wrapper-exception')
    finally:
        if armed:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous)
        if process is not None and not result['reaped']:
            terminate_and_reap('final-cleanup')
        if status is not None:
            process.returncode = os.waitstatus_to_exitcode(status)
            result['exit'] = process.returncode
        if usage is not None:
            result.update(cpu_ms=1000 * (usage.ru_utime + usage.ru_stime), rss_kib=usage.ru_maxrss)
        result['wall_ms'] = ((end or time.perf_counter_ns()) - (start or invocation_start)) / 1e6
        result['metric_scope'] = 'launch/input-through-reap' if start is not None else 'wrapper-failed-before-launch'
        # Always committed before any report/output decoding and before a
        # cancellation is re-raised; no environment values are included.
        write(log / 'process-attempt.json', result)
    if raised is not None:
        raise raised
    return result


def stats(values):
    # Missing values remain in the raw list and make the aggregate unavailable;
    # they are never silently filtered to create a successful subset.
    if any(v is None for v in values) or not values:
        return {'values': values, 'mean': None, 'median': None, 'p95': None, 'max': None}
    ordered = sorted(values)
    return {'values': values, 'mean': statistics.mean(values), 'median': statistics.median(values),
            'p95': ordered[math.ceil(.95 * len(ordered)) - 1], 'max': max(values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    protocol = json.loads(args.protocol.read_text())
    if not args.execute:
        print(json.dumps({'status': 'not-executed', 'protocol_sha256': sha(args.protocol)}, indent=2))
        return
    if protocol['format'] != 'zil-bounded-imports-runtime-quality/1' or protocol['status'] != 'prepared-not-run':
        raise ValueError('unsupported or unfrozen protocol')
    if protocol['iterations'] != 100000 or protocol['seed_period'] != 251 or protocol['acc_modulus'] != 1000003:
        raise ValueError('runtime model/loop tuning is forbidden')
    if protocol['gates']['per_case_B_over_A_mean_wall_max'] != 1.10 or protocol['gates']['per_case_Rust_over_B_mean_wall_min'] != .90:
        raise ValueError('runtime gates changed')
    orders = protocol['formal_orders']
    if len(orders) != 6 or len(set(orders)) != 6 or any(sorted(order) != ['A', 'B', 'R'] for order in orders):
        raise ValueError('six balanced three-arm permutations required')
    for position in range(3):
        if collections.Counter(order[position] for order in orders) != {'A': 2, 'B': 2, 'R': 2}:
            raise ValueError('unbalanced formal positions')
    for name, digest in protocol['tools'].items():
        if sha(Path(__file__).with_name(name)) != digest:
            raise ValueError(f'script changed since freeze: {name}')
    ref = protocol['reference_protocol']
    if sha(Path(ref['path'])) != ref['sha256']:
        raise ValueError('original corpus reference changed')
    for identity in protocol['binaries'].values():
        if sha(Path(identity['path'])) != identity['sha256']:
            raise ValueError('frozen compiler binary changed')
    clang = validate_clang_pin(protocol)
    if len(protocol['cases']) != 8:
        raise ValueError('all eight runtime cases are mandatory')
    for key, fixture in protocol['cases'].items():
        for name, digest in fixture['source_hashes'].items():
            if sha(args.protocol.parent / key / name) != digest:
                raise ValueError(f'runtime source changed: {key}/{name}')
        if fixture['runtime_oracle']['iterations'] != 100000 or fixture['runtime_oracle']['period'] != 251:
            raise ValueError('independent oracle identity mismatch')
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    ACTIVE.update(out=str(args.out), phase='execution-preflight')
    if not set(protocol['cpus']).issubset(os.sched_getaffinity(0)):
        raise ValueError('frozen CPUs unavailable')
    os.sched_setaffinity(0, protocol['cpus'])
    compiler = {arm: identity['path'] for arm, identity in protocol['binaries'].items()}
    version = subprocess.check_output([compiler['R'], '-vV'], text=True)
    (args.out / 'rustc-version.txt').write_text(version)
    if f"release: {protocol['fixed_toolchains_required']['rust']}" not in version:
        raise ValueError('Rust toolchain mismatch')
    shutil.copy2(args.protocol, args.out / 'protocol.json')
    write(args.out / 'execution-manifest.json', {'status': 'running', 'protocol_sha256': sha(args.protocol),
          'binaries': protocol['binaries'], 'source_claims': protocol['source_claims'],
          'build_receipts_required_for_source_binary_binding': True,
          'clang': clang,
          'frequency_start': frequency(protocol['cpus'], protocol['frequency'])})
    native_rows, compile_rows, case_summaries = [], [], []
    for case, fixture in protocol['cases'].items():
        ACTIVE.update(case=case, phase='case-compilation')
        case_root = args.out / case
        source = args.protocol.parent / case
        programs, artifacts = {}, {}
        for arm in ['A', 'B', 'R']:
            ACTIVE.update(arm=arm, phase='clean-O2-compilation')
            root = case_root / ('compile-' + arm)
            program = root / 'program'
            if arm == 'R':
                command = [compiler[arm], str(source / 'main.rs'), '--edition=2024', '-A', 'dead_code',
                           '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                           '-C', 'codegen-units=8', '-C', f'linker={clang["path"]}', '-o', str(program)]
            else:
                command = zil_command(compiler[arm], source, root, protocol, cache=False)
            metrics = measured(command, root, protocol['compile_timeout_seconds'])
            write(root / 'raw-metrics.json', metrics)
            ACTIVE['phase'] = 'compile-report-decode-and-validation'
            errors, evidence = [], None
            if metrics['exit'] != 0 or metrics['timeout']:
                errors.append('clean runtime program compilation failed')
            if arm != 'R' and not errors:
                report = json.loads((root / 'stdout').read_text())
                extra, evidence = validate_zil(report, fixture['total_files_per_language'], protocol)
                errors.extend(extra)
                if not errors:
                    artifacts[arm] = artifact_identity(report)
            record = {'case': case, 'arm': arm, **metrics, 'verify_preflight_evidence': evidence,
                      'valid': not errors, 'errors': errors, 'performance_sample': False}
            write(root / 'record.json', record)
            compile_rows.append(record)
            ACTIVE['compilations_completed'] = len(compile_rows)
            if errors:
                raise RuntimeError(f'{case}/{arm}: {errors}')
            programs[arm] = {'path': str(program), 'sha256': sha(program)}
        cross_arm = {key: artifacts['A'][key] == artifacts['B'][key]
                     for key in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']}
        write(case_root / 'artifact-classification.json', {'arms': artifacts, 'cross_arm_equal': cross_arm,
              'code_difference_allowed': True, 'independent_function_and_runtime_quality_gates_required': True})
        vectors = {v['stdin_hex']: v['stdout_hex'] for v in fixture['native_vectors']}
        schedule = [('extra-functional', index, arm, byte)
                    for byte in protocol['extra_functional_stdin_hex']
                    for index, arm in enumerate(protocol['warmup_order'])]
        schedule += [('warmup', 0, arm, protocol['stdin_timing_hex']) for arm in protocol['warmup_order']]
        schedule += [('formal', round_index, arm, protocol['stdin_timing_hex'])
                     for round_index, order in enumerate(orders) for arm in order]
        for position, (kind, round_index, arm, byte) in enumerate(schedule):
            ACTIVE.update(phase='native-' + kind, arm=arm, round=round_index, position=position)
            identity = programs[arm]
            if sha(Path(identity['path'])) != identity['sha256']:
                raise ValueError('native artifact changed after clean compilation')
            log = case_root / 'native' / f'{position:03d}-{kind}-{round_index}-{arm}-{byte}'
            before_frequency = frequency(protocol['cpus'], protocol['frequency'])
            metrics = measured([identity['path']], log, protocol['native_timeout_seconds'], bytes.fromhex(byte))
            write(log / 'raw-metrics.json', metrics)
            ACTIVE['phase'] = 'native-output-decode-and-validation'
            stdout, stderr = (log / 'stdout').read_bytes(), (log / 'stderr').read_bytes()
            valid = metrics['exit'] == 0 and not metrics['timeout'] and not stderr and stdout.hex() == vectors[byte]
            row = {'case': case, 'kind': kind, 'round': round_index, 'position': position, 'arm': arm,
                   'stdin_hex': byte, 'expected_stdout_hex': vectors[byte], 'stdout_hex': stdout.hex(),
                   'stderr_hex': stderr.hex(), 'artifact_sha256': identity['sha256'],
                   'frequency_before': before_frequency, **metrics, 'valid': valid}
            write(log / 'record.json', row)
            native_rows.append(row)
            with (args.out / 'native-rows.jsonl').open('a') as stream:
                stream.write(json.dumps(row) + '\n')
            ACTIVE['native_rows_written'] = len(native_rows)
        arms = {}
        for arm in ['A', 'B', 'R']:
            formal = [r for r in native_rows if r['case'] == case and r['arm'] == arm and r['kind'] == 'formal']
            if len(formal) != 6:
                raise RuntimeError('missing formal samples; no partial gate')
            arms[arm] = {metric: stats([r[metric] for r in formal]) for metric in ['wall_ms', 'cpu_ms', 'rss_kib']}
        a, b, rust = (arms[arm]['wall_ms']['mean'] for arm in ['A', 'B', 'R'])
        ratios = {'B_over_A': b / a if a and b else None, 'Rust_over_B': rust / b if rust and b else None}
        all_valid = all(r['valid'] for r in native_rows if r['case'] == case)
        gate = all_valid and ratios['B_over_A'] is not None and ratios['Rust_over_B'] is not None \
            and ratios['B_over_A'] <= 1.10 and ratios['Rust_over_B'] >= .90
        summary = {'case': case, 'all_native_rows_valid': all_valid, 'arms': arms, 'ratios': ratios,
                   'gate_pass': gate, 'cross_arm_code_identity': cross_arm,
                   'long_tails_and_failures_removed': 0}
        case_summaries.append(summary)
        write(case_root / 'runtime-summary.json', summary)
    ACTIVE['phase'] = 'final-gates'
    formal_count = sum(r['kind'] == 'formal' for r in native_rows)
    if len(compile_rows) != 24 or len(native_rows) != 216 or formal_count != 144:
        raise RuntimeError('nonzero complete fixed execution counts required')
    geomeans = {key: math.exp(statistics.mean(math.log(c['ratios'][key]) for c in case_summaries))
                if all(c['ratios'][key] is not None for c in case_summaries) else None
                for key in ['B_over_A', 'Rust_over_B']}
    passed = all(c['gate_pass'] for c in case_summaries)
    write(args.out / 'summary.json', {'format': 'zil-bounded-imports-runtime-result/1',
          'status': 'Pass' if passed else 'Failed', 'gate_pass': passed,
          'protocol_sha256': sha(args.protocol), 'binaries': protocol['binaries'],
          'clang': clang,
          'source_claims': protocol['source_claims'],
          'reference_protocol_sha256': protocol['reference_protocol']['sha256'],
          'counts': {'clean_compilations': len(compile_rows), 'native_total': len(native_rows), 'native_formal': formal_count},
          'cases': case_summaries, 'geomeans_additional_only': geomeans,
          'failed_cases': [c['case'] for c in case_summaries if not c['gate_pass']],
          'all_native_outputs_valid': all(r['valid'] for r in native_rows), 'outliers_removed': 0})
    manifest = json.loads((args.out / 'execution-manifest.json').read_text())
    manifest.update(status='Pass' if passed else 'Failed', completed=True)
    write(args.out / 'execution-manifest.json', manifest)
    print(json.dumps({'status': manifest['status'], 'out': str(args.out)}, indent=2))
    if not passed:
        raise SystemExit(1)


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        if ACTIVE.get('out'):
            out = Path(ACTIVE['out'])
            failure = {**ACTIVE, 'status': 'Incomplete' if isinstance(error, KeyboardInterrupt) else 'Failed',
                       'time_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                       'exception': {'type': type(error).__name__, 'message': str(error)},
                       'traceback': traceback.format_exc(), 'remaining_work': 'not executed; never counted Pass'}
            write(out / 'fatal-failure.json', failure)
            manifest_path = out / 'execution-manifest.json'
            manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
            manifest.update(status=failure['status'], completed=False, failure_receipt='fatal-failure.json')
            write(manifest_path, manifest)
        raise

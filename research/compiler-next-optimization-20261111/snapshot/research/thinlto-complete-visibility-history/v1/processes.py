"""Owned request sessions, inherited tool groups, raw-before-cleanup receipts."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

def measured(command, log, timeout, data=None, *, own_group=True):
    log = Path(log)
    log.mkdir(parents=True, exist_ok=False)
    group = os.getpgrp()
    if not own_group and not (group == os.getsid(0) == os.getppid()):
        raise RuntimeError('inner tool must be direct child of the owned ZIL session leader')
    result = {'command': command, 'exit': None, 'timeout': False, 'exception': None,
              'cpu_ms': None, 'rss_kib': None, 'wall_ms': None, 'reaped': False,
              'scope': 'owned-request-session' if own_group else 'inherited-owned-ZIL-group',
              'cleanup': [], 'owned_group': None, 'residual_descendants': False}
    process, usage, status, raised = None, None, None, None
    previous = signal.getsignal(signal.SIGALRM)
    start = time.perf_counter_ns()
    def expired(*_):
        raise TimeoutError('frozen process deadline')
    try:
        if data is not None:
            (log / 'stdin').write_bytes(data)
        with (log / 'stdout').open('wb') as stdout, (log / 'stderr').open('wb') as stderr:
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr,
                stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                start_new_session=own_group)
            result['owned_group'] = process.pid if own_group else group
            signal.signal(signal.SIGALRM, expired)
            signal.setitimer(signal.ITIMER_REAL, timeout)
            if data is not None:
                try:
                    process.stdin.write(data)
                    process.stdin.close()
                except BrokenPipeError:
                    result['stdin_broken_pipe'] = True
            _, status, usage = os.wait4(process.pid, 0)
            result['reaped'] = True
    except BaseException as error:
        raised = error
        result['timeout'] = isinstance(error, TimeoutError)
        result['exception'] = {'type': type(error).__name__, 'message': str(error)}
    finally:
        end = time.perf_counter_ns()
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        if not own_group and raised is not None:
            # SIGKILL includes this wrapper: persist facts before signalling.
            # The runner is outside this group and performs the final wait4.
            result['cleanup'].append({'action': 'SIGKILL-inherited-owned-request-group-after-raw',
                                      'group': group, 'parent': os.getppid(),
                                      'direct_parent_session_leader_verified': True})
            result['wall_ms'] = (end - start) / 1e6
            raw = json.dumps(result, indent=2) + '\n'
            (log / 'process-attempt.json').write_text(raw)
            (log / 'raw-metrics.json').write_text(raw)
            os.killpg(group, signal.SIGKILL)
            raise RuntimeError('owned-group kill unexpectedly returned')
        if own_group and process is not None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
                result['cleanup'].append({'action': 'SIGKILL-owned-request-group', 'group': process.pid})
                if result['reaped'] and raised is None:
                    result['residual_descendants'] = True
            except ProcessLookupError:
                result['cleanup'].append({'action': 'owned-request-group-empty', 'group': process.pid})
            if not result['reaped']:
                _, status, usage = os.wait4(process.pid, 0)
                result['reaped'] = True
        if status is not None:
            result['exit'] = os.waitstatus_to_exitcode(status)
            process.returncode = result['exit']
        if usage is not None:
            result.update(cpu_ms=1000 * (usage.ru_utime + usage.ru_stime), rss_kib=usage.ru_maxrss)
        result['wall_ms'] = (end - start) / 1e6
        (log / 'process-attempt.json').write_text(json.dumps(result, indent=2) + '\n')
    if raised is not None:
        raise raised
    return result

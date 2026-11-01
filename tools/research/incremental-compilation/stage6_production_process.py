#!/usr/bin/env python3
"""Linux ownership-bounded cleanup for a coordinator's nested process groups."""
import ctypes
import errno
import os
from pathlib import Path
import signal
import subprocess
import time


def enable_subreaper():
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:  # Linux PR_SET_CHILD_SUBREAPER.
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))
    # A missing proc interface is a capability failure, never an empty tree.
    owned_descendants()


def process_identity(pid):
    try:
        text = Path(f'/proc/{pid}/stat').read_text(errors='surrogateescape')
        fields = text[text.rfind(')') + 2:].split()
        return {'pid': pid, 'parent_pid': int(fields[1]), 'process_group': int(fields[2]),
                'start_ticks': int(fields[19]), 'state': fields[0]}
    except OSError as error:
        if error.errno in [errno.ENOENT, errno.ESRCH]:
            return None
        raise


def owned_descendants():
    """Read a PPID table, then select only this process's descendant closure.

    This environment exposes /proc/PID/stat but not task/TID/children. Reading
    unrelated stat records gives relationships only; unrelated PIDs never enter
    the kill set. Missing/unreadable required proc capabilities raise an error.
    """
    table = {}
    for path in Path('/proc').iterdir():
        if path.name.isdecimal():
            value = process_identity(int(path.name))
            if value is not None:
                table[value['pid']] = value
    own_pid = os.getpid()
    if own_pid not in table:
        raise RuntimeError('cannot discover own process through /proc stat')
    selected = {own_pid}
    while True:
        added = {pid for pid, value in table.items() if value['parent_pid'] in selected} - selected
        if not added:
            break
        selected.update(added)
    return {pid: table[pid] for pid in selected if pid != own_pid}


def kill_owned(identities):
    killed = []
    for pid, captured in identities.items():
        current = process_identity(pid)
        if current is not None and current['start_ticks'] == captured['start_ticks'] and current['state'] != 'Z':
            try:
                os.kill(pid, signal.SIGKILL)
                killed.append(captured)
            except ProcessLookupError:
                pass
    return killed


def interrupt_and_reap(child, grace_seconds=10):
    """Allow runner cleanup, then kill only owned descendants including new PGs.

    Subreaping keeps orphaned grandchildren owned after a runner dies. PID/start
    identities prevent a recorded PID from targeting a subsequently reused PID.
    All processes here descend from this coordinator, not from another agent.
    """
    captured = {}
    record = {'runner_pid': child.pid, 'captured': [], 'killed': [],
              'all_owned_processes_reaped': False, 'no_running_descendants': False}
    try:
        captured = owned_descendants()
        record['captured'] = list(captured.values())
        if child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGINT)
            except ProcessLookupError:
                pass
            try:
                child.wait(timeout=grace_seconds)
            except subprocess.TimeoutExpired:
                record['grace_expired'] = True
        captured.update(owned_descendants())
        record['killed'].extend(kill_owned(captured))
        # Direct-child cleanup is unconditional even if descendant discovery
        # was incomplete. A null Popen return code can never count as success.
        if child.poll() is None:
            child.kill()
        child.wait(timeout=5)
        deadline, empty_reads = time.monotonic() + 5, 0
        while True:
            captured.update(owned_descendants())
            record['killed'].extend(kill_owned(captured))
            while True:
                try:
                    pid, _ = os.waitpid(-1, os.WNOHANG)
                except ChildProcessError:
                    break
                if pid == 0:
                    break
            remaining = owned_descendants()
            empty_reads = empty_reads + 1 if not remaining else 0
            if empty_reads >= 2 or time.monotonic() >= deadline:
                record['remaining'] = list(remaining.values())
                record['all_owned_processes_reaped'] = empty_reads >= 2 and child.returncode is not None
                record['no_running_descendants'] = not [value for value in remaining.values() if value['state'] != 'Z']
                break
            time.sleep(0.01)
    except BaseException as error:
        record['cleanup_error'] = repr(error)
        # Preserve failure instead of interpreting lack of discovery as empty.
        record['all_owned_processes_reaped'] = False
        record['no_running_descendants'] = False
        if child.poll() is None:
            try:
                child.kill()
                child.wait(timeout=5)
            except BaseException as direct_error:
                record['direct_cleanup_error'] = repr(direct_error)
    record['runner_exit'] = child.returncode
    return record

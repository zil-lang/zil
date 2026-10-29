#!/usr/bin/env python3
"""Record the exact CP04 candidate and each independently preserved cargo step."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path('/workspace/scratch/037bacfed72a')
SOURCE = ROOT / 'cperf-ssa'
LOGS = ROOT / 'cp04-logs'
WRAPPER = ROOT / 'run-tool.py'
TARGET = ROOT / 'targets/cp04'


def git(*args):
    return subprocess.check_output(['git', *args], cwd=SOURCE, text=True).strip()


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


label = sys.argv[1]
command = ['python', str(WRAPPER), *sys.argv[2:]]
log = LOGS / f'{label}.log'
receipt = LOGS / f'{label}.json'
if log.exists() or receipt.exists():
    raise SystemExit('Refusing to overwrite an existing step')
environment = os.environ.copy()
environment['CARGO_TARGET_DIR'] = str(TARGET)
names = [
    'PATH', 'LD_LIBRARY_PATH', 'ZIL_LLVM_LIB_DIR', 'ZIL_CLANG',
    'CARGO_HOME', 'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL',
    'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_BUILD_JOBS',
    'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTDOCFLAGS',
]
probe = 'import os,json; print(json.dumps({k:os.environ.get(k) for k in ' + repr(names) + '}))'
effective_environment = json.loads(subprocess.check_output(
    ['python', str(WRAPPER), 'python', '-c', probe], cwd=SOURCE,
    env=environment, text=True,
))
data = {
    'source_commit': git('rev-parse', 'HEAD'),
    'source_tree': git('rev-parse', 'HEAD^{tree}'),
    'crates_tree': git('rev-parse', 'HEAD:crates'),
    'status_before': git('status', '--porcelain=v1'),
    'cwd': str(SOURCE),
    'argv': command,
    'environment': effective_environment,
    'wrapper': str(WRAPPER),
    'wrapper_sha256': hashlib.sha256(WRAPPER.read_bytes()).hexdigest(),
    'started_at': now(),
    'log': str(log),
    'state': 'running',
}
receipt.write_text(json.dumps(data, indent=2) + '\n')
start = time.monotonic()
print(f'Started {label}: {data["source_commit"]}', flush=True)
with log.open('wb') as output:
    result = subprocess.run(command, cwd=SOURCE, env=environment,
                            stdout=output, stderr=subprocess.STDOUT)
data.update({
    'ended_at': now(),
    'wall_seconds': time.monotonic() - start,
    'exit': result.returncode,
    'state': 'complete',
    'source_commit_after': git('rev-parse', 'HEAD'),
    'status_after': git('status', '--porcelain=v1'),
    'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(),
    'log_bytes': log.stat().st_size,
})
receipt.write_text(json.dumps(data, indent=2) + '\n')
print(json.dumps({'receipt': str(receipt), 'exit': result.returncode,
                  'wall_seconds': data['wall_seconds'],
                  'log_sha256': data['log_sha256']}), flush=True)
print('\n'.join(log.read_text(errors='replace').splitlines()[-35:]), flush=True)
raise SystemExit(result.returncode)

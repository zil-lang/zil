#!/usr/bin/env python3
"""Sequential, scoped CP27 correctness tests. Invoke only after quiet-window release."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path('/workspace/scratch/4ab361722127/zil-cp27')
OUT = Path('/tmp/zil-review/cp27-author-tests')
OUT.mkdir(exist_ok=True)
env = os.environ.copy()
env.update({
    'CARGO_TARGET_DIR': '/workspace/scratch/4ab361722127/target-cp27',
    'CARGO_PROFILE_DEV_DEBUG': '0',
    'CARGO_PROFILE_TEST_DEBUG': '0',
    'CARGO_INCREMENTAL': '0',
    'CARGO_BUILD_JOBS': '2',
})
commands = [
    ('query-unit', ['cargo', 'test', '--offline', '--locked', '-p', 'zil-driver', '--lib', 'query_cache::tests']),
    ('object-unit', ['cargo', 'test', '--offline', '--locked', '-p', 'zil-backend-llvm', '--lib', 'object_cache::tests']),
    ('query-native-collision', ['cargo', 'test', '--offline', '--locked', '-p', 'zil-driver', '--test', 'cache_publication_cp27']),
    ('query-existing-cp12', ['cargo', 'test', '--offline', '--locked', '-p', 'zil-driver', '--test', 'query_cache_cp12']),
]
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def capture(command):
    return subprocess.check_output(command, cwd=ROOT, env=env, text=True).strip()
record = {
    'task': 'CP27',
    'scope': 'author targeted correctness; not a full workspace or performance gate',
    'baseline': 'd8872b52f5a215c2efd33aaadba5083f471e0bc2',
    'worktree_head_at_test': capture(['git', 'rev-parse', 'HEAD']),
    'environment_setup': 'source /workspace/scratch/4ab361722127/.toolchain/env.sh',
    'environment': {k: env.get(k) for k in [
        'CARGO_TARGET_DIR', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG',
        'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'ZIL_LLVM_LIB_DIR', 'LD_LIBRARY_PATH',
        'ZIL_CLANG', 'ZIL_LINUX_LINKER', 'RUSTFLAGS', 'RUSTDOCFLAGS']},
    'tools': {'rustc': capture(['rustc', '-Vv']), 'cargo': capture(['cargo', '-V'])},
    'source_sha256': {p: sha(ROOT/p) for p in [
        'crates/zil-driver/src/query_cache.rs',
        'crates/zil-backend-llvm/src/object_cache.rs',
        'crates/zil-driver/tests/cache_publication_cp27.rs',
        'crates/zil-driver/tests/query_cache_cp12.rs']},
    'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'rows': [],
}
receipt = OUT/'receipt.json'
for label, command in commands:
    print(f'START {label}: {command}', flush=True)
    log = OUT/f'{label}.log'
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with log.open('wb') as f:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT)
    record['rows'].append({
        'label': label, 'command': command, 'cwd': str(ROOT), 'started_utc': started,
        'completed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'exit_code': result.returncode, 'log': str(log), 'log_sha256': sha(log),
    })
    record['status'] = 'Running' if result.returncode == 0 else 'Failed'
    receipt.write_text(json.dumps(record, indent=2, ensure_ascii=False)+'\n')
    print(f'DONE {label}: {result.returncode}; {log}', flush=True)
    if result.returncode != 0:
        print(log.read_text()[-10000:], flush=True)
        sys.exit(result.returncode)
record['status'] = 'Pass'
record['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
receipt.write_text(json.dumps(record, indent=2, ensure_ascii=False)+'\n')
print(f'RECEIPT {receipt}', flush=True)

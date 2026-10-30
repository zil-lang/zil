#!/usr/bin/env python3
"""Build and freeze the reviewed CP27 source, without timing compiler workloads."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import sys

CP27_WORKSPACE = Path('/workspace/scratch/4ab361722127')
ROOT = CP27_WORKSPACE/'zil-cp27'
SOURCE = 'd9a71f603d73c1df4a367189cc5b41ca15bf7d70'
TREE = 'efd735f8821637c1876708cd19d5b649fa70530c'
OUT = Path('/tmp/zil-review')
env = os.environ.copy()
env.update({'CARGO_TARGET_DIR': str(CP27_WORKSPACE/'build-cp27'), 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '2'})
command = ['cargo', 'build', '--offline', '--locked', '--release', '-p', 'zil-cli']
def capture(command):
    return subprocess.check_output(command, cwd=ROOT, env=env, text=True).strip()
def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1024*1024), b''):
            digest.update(chunk)
    return digest.hexdigest()
def meta(path):
    s = path.stat()
    return {'dev': s.st_dev, 'ino': s.st_ino, 'nlink': s.st_nlink,
            'size': s.st_size, 'mode': s.st_mode,
            'mtime_ns': s.st_mtime_ns, 'ctime_ns': s.st_ctime_ns}
assert capture(['git', 'rev-parse', 'HEAD']) == SOURCE
assert capture(['git', 'rev-parse', 'HEAD:crates']) == TREE
assert not capture(['git', 'status', '--porcelain', '--', 'crates', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tools/llvm/version.txt'])
protected = [CP27_WORKSPACE/f'build-{name}/release/zil' for name in ['base', 'cp21', 'cp23', 'cp26']]
tools = {name: Path(shutil.which(name, path=env['PATH'])).resolve() for name in ['rustc', 'cargo', 'clang', 'ld.lld']}
tools['libLLVM'] = (Path(env['ZIL_LLVM_LIB_DIR'])/'libLLVM-23.so').resolve()
protected.extend(tools.values())
before = {str(path): meta(path) for path in protected}
record = {
    'task': 'CP27', 'scope': 'release construction only; no benchmark',
    'source_commit': SOURCE, 'crates_tree': TREE,
    'command': command, 'cwd': str(ROOT),
    'environment_setup': 'source /workspace/scratch/4ab361722127/.toolchain/env.sh',
    'environment': {k: env.get(k) for k in ['CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'ZIL_LLVM_LIB_DIR', 'LD_LIBRARY_PATH', 'ZIL_CLANG', 'ZIL_LINUX_LINKER', 'RUSTFLAGS', 'RUSTDOCFLAGS']},
    'toolchain': {name: {'path': str(path), 'sha256': sha(path), 'metadata': meta(path)} for name,path in tools.items()},
    'tool_versions': {'rustc': capture(['rustc', '-Vv']), 'cargo': capture(['cargo', '-V']), 'clang': capture(['clang', '--version']), 'lld': capture(['ld.lld', '--version'])},
    'protected_before': before,
    'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
}
log = OUT/'CP27-release-build.log'
print(f'START {command}; log={log}', flush=True)
with log.open('wb') as f:
    result = subprocess.run(command, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT)
record.update({'completed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'exit_code': result.returncode, 'log': str(log), 'log_sha256': sha(log)})
after = {str(path): meta(path) for path in protected}
record['protected_after'] = after
record['protected_metadata_unchanged'] = before == after
record['status'] = 'Built' if result.returncode == 0 else 'BuildFailed'
if result.returncode == 0:
    binary = CP27_WORKSPACE/'build-cp27/release/zil'
    record['binary'] = {'path': str(binary), 'sha256': sha(binary), 'metadata': meta(binary)}
receipt = OUT/'CP27-release-build.json'
receipt.write_text(json.dumps(record, indent=2, ensure_ascii=False)+'\n')
print(json.dumps({'status':record['status'], 'exit_code':result.returncode,
                  'binary':record.get('binary'), 'protected_metadata_unchanged':before==after,
                  'receipt':str(receipt)}, ensure_ascii=False), flush=True)
if result.returncode:
    print(log.read_text()[-10000:], flush=True)
assert before == after, 'frozen binary or tool metadata changed during build'
sys.exit(result.returncode)

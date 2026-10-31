#!/usr/bin/env bash
set -euo pipefail

if (( $# < 4 )); then
  echo 'usage: build_plugin.sh LLVM_PREFIX HEADERS_PREFIX OUTPUT_SO SOURCE.cpp [...]' >&2
  exit 2
fi
jt_llvm_prefix=$1
jt_headers_prefix=$2
jt_output=$3
shift 3

# One compiler driver, serial frontends: never more than two build jobs.
# The official opt statically links LLVM and exports its plugin ABI. Do not
# link a second libLLVM into this DSO; host opt must resolve LLVM references.
python3 - "$jt_llvm_prefix" "$jt_headers_prefix" "$jt_output" "$@" <<'PY'
import hashlib
import json
import shlex
import subprocess
import sys
from pathlib import Path

llvm, headers, output = map(Path, sys.argv[1:4])
sources = list(map(Path, sys.argv[4:]))
output.parent.mkdir(parents=True, exist_ok=True)
if output.exists():
    raise SystemExit("Refusing to replace an existing plugin")

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def config(*args):
    return subprocess.check_output([str(llvm / 'bin/llvm-config'), *args], text=True).strip()

assert config('--version') == '23.1.4'
assert config('--has-rtti') == 'NO'
assert config('--assertion-mode') == 'OFF'
header_receipt = json.loads((headers / 'headers-receipt.json').read_text())
assert header_receipt['status'] == 'Pass'
assert header_receipt['release']['version'] == '23.1.4'
flags = [x for x in shlex.split(config('--cxxflags')) if not x.startswith('-I')]
command = [str(llvm / 'bin/clang++'), '-I' + str(headers / 'include'), *flags,
           '-O3', '-DNDEBUG', '-fPIC', '-shared', '-fuse-ld=lld',
           '-Wl,--build-id=sha1', *map(str, sources), '-o', str(output)]
receipt = {'status': 'building', 'command': command, 'build_jobs': 1,
           'llvm_resolution': 'unresolved plugin references resolve against host opt; no -lLLVM',
           'host_opt_sha256': sha(llvm / 'bin/opt'),
           'compiler_sha256': sha((llvm / 'bin/clang++').resolve()),
           'headers_receipt_sha256': sha(headers / 'headers-receipt.json'),
           'sources': {str(p): sha(p) for p in sources}}
receipt_path = output.with_suffix('.build.json')
receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
with output.with_suffix('.build.stdout').open('wb') as so, output.with_suffix('.build.stderr').open('wb') as se:
    result = subprocess.run(command, stdout=so, stderr=se)
receipt['returncode'] = result.returncode
receipt['status'] = 'Pass' if result.returncode == 0 else 'Fail'
if result.returncode == 0:
    dynamic = subprocess.check_output(['readelf', '-d', str(output)], text=True)
    output.with_suffix('.dynamic.txt').write_text(dynamic)
    if 'libLLVM' in dynamic:
        receipt['status'] = 'Fail'
        receipt['error'] = 'Plugin unexpectedly links a second LLVM runtime'
    receipt['plugin_sha256'] = sha(output)
receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps({'status': receipt['status'], 'receipt': str(receipt_path)}))
raise SystemExit(0 if receipt['status'] == 'Pass' else 1)
PY

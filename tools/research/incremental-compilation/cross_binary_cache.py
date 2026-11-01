#!/usr/bin/env python3
"""Prove a compiler upgrade cannot reuse the old noncanonical O2 object."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser(description=__doc__)
for name in ['baseline', 'candidate', 'out']:
    p.add_argument('--' + name, required=True, type=Path)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=False)
source = Path('examples/mainline/byte_io/main.zil').resolve()
data = b'hello, Zil!\n\0\x80'
rows = []
for arm, warm in [('baseline', False), ('baseline', True), ('candidate', False), ('candidate', True)]:
    compiler = getattr(a, arm).resolve()
    name = f'{arm}-{warm}'
    cmd = [str(compiler), 'build', str(source), '--backend', 'llvm-api', '--opt', 'O2',
           '--object-cache', str(a.out / 'objects'), '--json', '-o', str(a.out / 'program')]
    result = subprocess.run(cmd, capture_output=True, timeout=180)
    (a.out / (name + '.report.json')).write_bytes(result.stdout)
    (a.out / (name + '.stderr')).write_bytes(result.stderr)
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert report['status'] == 'Success' and report['verification_runs'] == 1
    cache = report['native_codegen']['object_cache']
    assert cache['enabled'] and cache['hits'] == int(warm) and cache['misses'] == int(not warm), cache
    native = subprocess.run([str(a.out / 'program')], input=data, capture_output=True, timeout=10)
    assert native.stdout == data.upper() and native.stderr == b''
    assert native.returncode == (2 if arm == 'baseline' else 0), (arm, native.returncode)
    rows.append({'arm': arm, 'warm': warm, 'command': cmd,
                 'compiler_sha256': hashlib.sha256(compiler.read_bytes()).hexdigest(),
                 'cache': cache, 'exit': native.returncode, 'stdout_hex': native.stdout.hex()})
(a.out / 'result.json').write_text(json.dumps({'source': str(source),
    'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'stdin_hex': data.hex(), 'rows': rows, 'status': 'Pass'}, indent=2) + '\n')
print('Pass: old miss/hit reproduce exit 2; new miss/hit preserve bytes and exit 0')

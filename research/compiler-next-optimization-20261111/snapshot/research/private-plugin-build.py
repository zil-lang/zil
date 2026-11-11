#!/usr/bin/env python3
"""One uniquely named research plugin build, bound to an exact clean Git tree."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

WORKSPACE = Path('/workspace/scratch/abf690ce880c')
sys.path.insert(0, str(WORKSPACE / 'research/thinlto-private-imports'))
from processes import measured


def ident(path):
    path = Path(os.path.abspath(path))
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--checkout', type=Path, required=True)
    q.add_argument('--commit', required=True)
    q.add_argument('--tree', required=True)
    q.add_argument('--out', type=Path, required=True)
    a = q.parse_args()
    a.checkout = a.checkout.resolve(strict=True)
    def git(*args):
        return subprocess.check_output(['git', '-C', str(a.checkout), *args], text=True).strip()
    if git('rev-parse', 'HEAD') != a.commit or git('rev-parse', 'HEAD^{tree}') != a.tree or git('status', '--porcelain=v1'):
        raise ValueError('plugin must build from the exact clean frozen checkout')
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    sdk = WORKSPACE / 'toolchain/llvm-23.1.4'
    source = a.checkout / 'research/compiler-thinlto-private-imports/PrivateImports.cpp'
    original = ident(source)
    compiler, plugin = sdk / 'bin/clang++', out / 'PrivateImports.so'
    command = [str(compiler), '-std=c++17', '-O2', '-shared', '-fPIC', '-fno-exceptions', '-fno-rtti',
               '-D_GLIBCXX_USE_CXX11_ABI=1', '-I' + str(sdk / 'include'),
               '-MMD', '-MF', str(out / 'dependencies.d'), str(source), '-o', str(plugin), '-ldl']
    os.sched_setaffinity(0, [0, 1])
    (out / 'plan.json').write_text(json.dumps({'command': command, 'source_commit': a.commit,
        'source_tree': a.tree, 'source': original, 'compiler': ident(compiler),
        'cpu_affinity': sorted(os.sched_getaffinity(0)), 'recipe': ident(__file__)}, indent=2) + '\n')
    result = measured(command, out / 'build', 180)
    passed = result['exit'] == 0 and not result['timeout'] and not result['exception'] and not result['residual_descendants']
    if ident(source) != original:
        raise ValueError('source changed while compiling plugin')
    needed = None
    if passed:
        dynamic = measured(['readelf', '-d', str(plugin)], out / 'dynamic-dependencies', 30)
        needed = (out / 'dynamic-dependencies/stdout').read_text()
        passed = dynamic['exit'] == 0 and not dynamic['timeout'] and 'libLLVM' not in needed
    binding = {'status': 'Pass' if passed else 'Failed', 'source_commit': a.commit,
               'source_tree': a.tree, 'source': original, 'compiler': ident(compiler),
               'plugin': ident(plugin) if plugin.is_file() else None,
               'build_log': ident(out / 'build/process-attempt.json'),
               'stdout': ident(out / 'build/stdout'), 'stderr': ident(out / 'build/stderr'),
               'plan': ident(out / 'plan.json'), 'command': command,
               'links_second_libLLVM': 'libLLVM' in needed if needed is not None else None,
               'sdk_archive_sha256': 'efdd1dbd089b99bfcfcde7b8eb1236163f2e88df5a981cca4f783ec9cb5e0378'}
    (out / 'binding.json').write_text(json.dumps(binding, indent=2) + '\n')
    print(json.dumps(binding, indent=2))
    if not passed:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

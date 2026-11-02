#!/usr/bin/env python3
"""Assert actual extracted cache methods against full relation scans and LLVM callbacks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def identity(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'bytes': path.stat().st_size}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--sources', type=Path, required=True)
    p.add_argument('--original-receipt', type=Path, required=True)
    p.add_argument('--indexed-receipt', type=Path, required=True)
    p.add_argument('--headers-prefix', type=Path, required=True)
    p.add_argument('--clang', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=False)
    fixture = Path(__file__).with_name('cache_control.cpp')
    manifest = {'scope': __doc__, 'fixture': identity(fixture),
                'runner': identity(Path(__file__)), 'clang': identity(a.clang),
                'source_manifest': identity(a.sources / 'source-manifest.json'),
                'headers_receipt': identity(a.headers_prefix / 'headers-receipt.json'),
                'test_only_source_adjustments': [
                    'Extract complete cache class, callback and method section; methods unchanged',
                    'Expose anonymous cache private fields for full-scan invariant assertions',
                    'Replace external cl::opt accessor with test bool selecting the same two modes',
                    'Compile extraction with assertions enabled; actual LLVM objects/callbacks come from identified DSO',
                ], 'arms': {}}
    rows = []
    for arm, receipt_path in [('original', a.original_receipt), ('indexed', a.indexed_receipt)]:
        dest = a.out / arm
        dest.mkdir()
        receipt = json.loads(receipt_path.read_text())
        assert receipt['status'] == 'Pass'
        library = Path(receipt['library']['path'])
        assert identity(library)['sha256'] == receipt['library']['sha256']
        source = a.sources / arm / 'LazyValueInfo.cpp'
        text = source.read_text()
        start = text.index('namespace {\n  /// A callback value handle updates the cache')
        end = text.index('namespace llvm {\nnamespace {\n/// An assembly annotator', start)
        cache = text[start:end]
        assert cache.count('class LazyValueInfoCache {\n') == 1
        exposed = cache.replace('class LazyValueInfoCache {\n', 'class LazyValueInfoCache {\npublic:\n')
        prefix = text[:text.index('#include')]
        includes = ''.join(line for line in text.splitlines(True) if line.startswith('#include '))
        extracted = prefix + includes + '\nusing namespace llvm;\nstatic bool PerPredRanges = false;\n' + exposed
        path = dest / 'cache_impl.inc'
        path.write_text(extracted)
        command = [str(a.clang), '-std=c++17', '-fno-exceptions', '-fno-rtti',
                   '-D_GLIBCXX_USE_CXX11_ABI=1', '-O1', '-UNDEBUG',
                   '-DLVI_INDEXED=' + str(int(arm == 'indexed')),
                   '-I' + str(a.headers_prefix / 'include'), '-I' + str(dest),
                   str(fixture), '-L' + str(library.parent), '-lLLVM-23', '-ldl',
                   '-Wl,-rpath,' + str(library.parent), '-o', str(dest / 'control')]
        arm_manifest = {'source': identity(source), 'extracted': identity(path),
                        'method_section_sha256': hashlib.sha256(cache.encode()).hexdigest(),
                        'library': receipt['library'], 'toolchain_receipt': identity(receipt_path),
                        'build_command': command}
        manifest['arms'][arm] = arm_manifest
        (a.out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        build = subprocess.run(command, capture_output=True, timeout=60)
        (dest / 'build.stdout').write_bytes(build.stdout)
        (dest / 'build.stderr').write_bytes(build.stderr)
        assert build.returncode == 0, (arm, build.stderr.decode(errors='replace'))
        arm_manifest['control_binary'] = identity(dest / 'control')
        env = os.environ.copy()
        env.pop('LD_PRELOAD', None)
        env.pop('LD_AUDIT', None)
        env['LD_LIBRARY_PATH'] = str(library.parent)
        for per_pred in ['false', 'true']:
            for mode in ['normal', 'reject-epoch']:
                label = per_pred + '-' + mode
                command = [str(dest / 'control'), per_pred, mode, str(library)]
                run = subprocess.run(command, env=env, capture_output=True, timeout=10)
                (dest / (label + '.stdout')).write_bytes(run.stdout)
                (dest / (label + '.stderr')).write_bytes(run.stderr)
                if mode == 'normal':
                    valid = run.returncode == 0 and not run.stderr and b'Pass perpred=' in run.stdout
                else:
                    valid = run.returncode == -6 and b'BlockNumberEpoch' in run.stderr
                rows.append({'arm': arm, 'per_pred': per_pred, 'mode': mode,
                             'command': command, 'exit': run.returncode, 'valid': valid,
                             'stdout_sha256': hashlib.sha256(run.stdout).hexdigest()})
                assert valid, (arm, label, run.stdout, run.stderr)
        (a.out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    comparisons = []
    for per_pred in ['false', 'true']:
        left = (a.out / 'original' / (per_pred + '-normal.stdout')).read_bytes()
        right = (a.out / 'indexed' / (per_pred + '-normal.stdout')).read_bytes()
        assert left == right
        comparisons.append({'per_pred': per_pred, 'all_checkpoint_outputs_equal': True})
    result = {'status': 'Pass', 'scope': 'Internal actual-cache implementation control, separate from public C API and ZIL source controls',
              'runs': rows, 'comparisons': comparisons,
              'normal_runs': 4, 'expected_epoch_rejections': 4,
              'checkpoints_per_normal_run': 13}
    (a.out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ['runs', 'comparisons']}))


if __name__ == '__main__':
    main()

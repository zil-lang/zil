#!/usr/bin/env python3
"""One bounded actual-API diagnostic for an explicitly identified combined DSO."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def identity(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'bytes': path.stat().st_size}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--receipt', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--zil', type=Path, required=True)
    source_group = p.add_mutually_exclusive_group(required=True)
    source_group.add_argument('--source-dir', type=Path)
    source_group.add_argument('--source', type=Path)
    p.add_argument('--expected-hex')
    p.add_argument('--source-receipt', type=Path, required=True)
    p.add_argument('--merge', choices=['off', 'after', 'before-and-after'], default='after')
    p.add_argument('--closed', action='store_true')
    p.add_argument('--probe-manifest', type=Path, required=True)
    p.add_argument('--probe-runner', type=Path, required=True)
    p.add_argument('--clang', type=Path, required=True)
    a = p.parse_args()
    assert not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT')
    assert a.clang.is_file() and os.access(a.clang, os.X_OK)
    clang_version = subprocess.check_output([str(a.clang), '--version'], text=True)
    assert 'clang version 23.1.4' in clang_version
    a.out.mkdir(parents=True, exist_ok=False)
    receipt = json.loads(a.receipt.read_text())
    assert receipt['status'] == 'Pass'
    library = Path(receipt['library']['path'])
    assert identity(library)['sha256'] == receipt['library']['sha256']
    source_receipt = json.loads(a.source_receipt.read_text())
    assert source_receipt['library']['sha256'] == receipt['library']['sha256']
    assert source_receipt['pipeline_identity_tag'].encode() in a.zil.read_bytes()
    probe = json.loads(a.probe_manifest.read_text())
    assert identity(Path(probe['probe']['path'])) == probe['probe']
    probe['llvm'] = identity(library)
    probe['scope'] = 'Existing timing-only probe; actual isolated combined DSO selected explicitly'
    probe['base_probe_manifest'] = identity(a.probe_manifest)
    probe['native_toolchain_receipt'] = identity(a.receipt)
    derived_probe = a.out / 'probe-manifest.json'
    derived_probe.write_text(json.dumps(probe, indent=2) + '\n')
    if a.source_dir:
        source = a.source_dir / 'main.zil'
        native_input = a.source_dir / 'input.bin'
        expected_path = a.source_dir / 'closed0-slp-on.row.json'
        expected_row = json.loads(expected_path.read_text())
        assert expected_row['valid'] and native_input.read_bytes() == bytes([7])
        expected = next(x['expected_hex'] for x in expected_row['native_controls'] if x['seed'] == 7)
    else:
        assert a.expected_hex is not None
        source, native_input, expected_path = a.source, None, None
        expected = a.expected_hex
        expected_row = {'native_controls': [{'seed': None, 'expected_hex': expected}]}
    command = [sys.executable, str(a.probe_runner), '--zil', str(a.zil),
               '--probe-manifest', str(derived_probe), '--source', str(source),
               '--out', str(a.out / 'request'), '--merge', a.merge,
               '--expected-hex', expected,
               '--timeout', '180']
    if native_input:
        command += ['--native-input', str(native_input)]
    if a.closed:
        command += ['--closed']
    env = os.environ.copy()
    env['LD_LIBRARY_PATH'] = str(library.parent)
    env['ZIL_CLANG'] = str(a.clang.absolute())
    env.pop('ZIL_PASS_TIMING_SLP', None)
    manifest = {'scope': __doc__, 'runner': identity(Path(__file__)),
                'toolchain': identity(a.receipt), 'library': receipt['library'],
                'compiler': identity(a.zil), 'source': identity(source),
                'input': identity(native_input) if native_input else None,
                'expected_row': identity(expected_path) if expected_path else expected_row,
                'source_receipt': identity(a.source_receipt),
                'merge': a.merge, 'closed': a.closed,
                'probe_runner': identity(a.probe_runner), 'probe': probe,
                'clang': identity(a.clang), 'clang_version': clang_version,
                'rss_method': 'os.wait4 of this exact probe child, including its waited descendants',
                'command': command,
                'environment': {'LD_LIBRARY_PATH': env['LD_LIBRARY_PATH'],
                                'ZIL_CLANG': env['ZIL_CLANG'],
                                'LD_PRELOAD': None, 'LD_AUDIT': None,
                                'ZIL_PASS_TIMING_SLP': None},
                'timing_scope': 'Concurrent functional diagnostic, not formal performance'}
    (a.out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    with (a.out / 'runner.stdout').open('wb') as stdout, (a.out / 'runner.stderr').open('wb') as stderr:
        run = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr,
                               start_new_session=True)
        deadline = time.monotonic() + 210
        while True:
            pid, status, usage = os.wait4(run.pid, os.WNOHANG)
            if pid:
                break
            if time.monotonic() >= deadline:
                os.killpg(run.pid, signal.SIGKILL)
                pid, status, usage = os.wait4(run.pid, 0)
                break
            time.sleep(0.05)
        run.returncode = os.waitstatus_to_exitcode(status)
    resources = {'max_rss_kib': usage.ru_maxrss, 'user_seconds': usage.ru_utime,
                 'system_seconds': usage.ru_stime,
                 'scope': 'waited probe runner and compiler descendants before native controls'}
    (a.out / 'resources.json').write_text(json.dumps(resources, indent=2) + '\n')
    result_path = a.out / 'request/result.json'
    result = json.loads(result_path.read_text()) if result_path.exists() else {}
    native_controls = []
    if run.returncode == 0 and result.get('valid'):
        for row in expected_row['native_controls']:
            seed = row['seed']
            data = b'' if seed is None else native_input.read_bytes() if seed == 7 else bytes([seed])
            native = subprocess.run([str(a.out / 'request/program')], input=data,
                                    capture_output=True, timeout=15)
            valid = native.returncode == 0 and not native.stderr and native.stdout.hex() == row['expected_hex']
            native_controls.append({'seed': seed, 'input_hex': data.hex(),
                                    'exit': native.returncode,
                                    'stdout_hex': native.stdout.hex(),
                                    'stderr_hex': native.stderr.hex(),
                                    'expected_hex': row['expected_hex'], 'valid': valid})
    rss = resources['max_rss_kib']
    tables_path = a.out / 'request/pass-tables.json'
    tables = json.loads(tables_path.read_text()) if tables_path.exists() else []
    objects = (result.get('native_codegen') or {}).get('object_reports') or []
    stderr = (a.out / 'request/driver.stderr').read_text() if (a.out / 'request/driver.stderr').exists() else ''
    final = {'runner_exit': run.returncode, 'result': result,
             'native_controls': native_controls,
             'max_rss_kib': rss,
             'pass_cpu_seconds': {r['name']: r['cpu_seconds'] for r in tables
                                  if r['name'] in ['CorrelatedValuePropagationPass', 'JumpThreadingPass',
                                                   'InstCombinePass', 'SLPVectorizerPass']},
             'valid': run.returncode == 0 and result.get('valid') and bool(objects)
                      and all(r['verify_before_after'] and r['input_kind'] == 'builder' for r in objects)
                      and len(native_controls) == len(expected_row['native_controls'])
                      and all(r['valid'] for r in native_controls)
                      and rss > 0 and '[zil-api-pass-timing] vectorize-slp=' not in stderr}
    (a.out / 'result.json').write_text(json.dumps(final, indent=2) + '\n')
    print(json.dumps({k: v for k, v in final.items() if k not in ['result', 'native_controls']}))
    raise SystemExit(0 if final['valid'] else 1)


if __name__ == '__main__':
    main()

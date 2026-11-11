"""Frozen research support; importing this module never launches a process."""
import hashlib
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / 'vendor'))
from runtime_gate import stats
from processes import measured as measure_process
from runtime_fixture import generate_runtime
from fixture import generate

ROOT = Path(__file__).resolve().parent
MAIN = '99035931d358add0eaed6d4535035c38413701dd'
EXPORTER = 'eb5b150903b67d8b9011f6adc2d3ac67b1c419a6'
ORDERS = ['ABR', 'BRA', 'RAB', 'ARB', 'RBA', 'BAR']
OWN_GROUP = True

def measured(command, log, timeout, data=None):
    return measure_process(command, log, timeout, data, own_group=OWN_GROUP)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')

def identity(path):
    # argv[0] selects the driver role for ld.lld/lld and rustc/rustup.
    # Keep the absolute invocation path; reading bytes follows symlinks for SHA.
    path = Path(os.path.abspath(path))
    path.resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path)}

def scripts():
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(ROOT.rglob('*.py'))}

def validate(p):
    if p['format'] != 'zil-thinlto-pure-runtime/1' or p['status'] != 'prepared-not-run':
        raise ValueError('unknown protocol')
    if p['scripts'] != scripts():
        raise ValueError('script freeze changed')
    for item in p['tools'].values():
        if identity(item['path']) != item:
            raise ValueError('tool bytes/path changed')
    for arm, item in p['build_receipts'].items():
        if identity(item['path']) != item:
            raise ValueError('build binding receipt changed')
        binding = json.loads(Path(item['path']).read_text())
        compiler = p['tools']['baseline' if arm == 'A' else 'exporter']
        if (binding.get('source_commit') != p['source_claims'][arm]
                or binding.get('binary_sha256') != compiler['sha256']
                or str(Path(binding['binary']).resolve()) != compiler['path']
                or binding.get('release_build_status') != 'Pass'):
            raise ValueError('source and binary are not bound by successful build')
        expected_tree = '1c16744d838064cb1c687ba56ef3a0d17ed3130c' if arm == 'A' else '1da99d09dbeb53e59f8df4572f4bc5d010ff5929'
        if binding.get('source_tree') != expected_tree or sha(binding['build_log']) != binding['build_log_sha256']:
            raise ValueError('source tree or build log binding changed')
    if p['source_claims'] != {'A': MAIN, 'B': EXPORTER}:
        raise ValueError('source identity changed')
    if p['cases_requested'] != [64, 256] or p['iterations'] != 100000 or p['orders'] != ORDERS:
        raise ValueError('case or schedule changed')
    if p['limits'] != {'B_over_A_max': 1.10, 'R_over_B_min': .90}:
        raise ValueError('runtime gate changed')
    if p['jobs'] != 2 or len(p['cpus']) != 2 or len(set(p['cpus'])) != 2:
        raise ValueError('two CPUs and two jobs required')
    if not set(p['cpus']).issubset(os.sched_getaffinity(0)):
        raise ValueError('frozen CPUs unavailable')

def run(command, log, timeout=600, data=None):
    result = measured([str(v) for v in command], Path(log), timeout, data)
    write(Path(log) / 'raw-metrics.json', result)
    if result['exit'] != 0 or result['timeout'] or result['exception'] or result['residual_descendants']:
        raise RuntimeError(f'process failed, raw already saved: {log}')
    return result

def preflight(path, expected_units=8):
    x = json.loads(Path(path).read_text())
    if x.get('format') != 'zil-thinlto-export-preflight/1' or x.get('request') != 'O2+incremental-o2+after':
        raise ValueError('missing actual current exporter receipt')
    if x.get('backend') != 'clang' or x.get('imports') != 'disabled-for-llvm-thinlto':
        raise ValueError('wrong exporter policy')
    if x.get('actual_units') != expected_units or len(x.get('units', [])) != expected_units:
        raise ValueError('incomplete machine unit preflight')
    if x.get('original_preflight_ns', 0) <= 0 or x.get('partition_input_preflight_ns', 0) <= 0:
        raise ValueError('full original/partition model preflight absent')
    if [v['unit'] for v in x['units']] != list(range(expected_units)) or any(v['preflight_ns'] <= 0 for v in x['units']):
        raise ValueError('unit preflight absent')
    return x

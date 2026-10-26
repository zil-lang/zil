#!/usr/bin/env python3
"""Freeze real TypedIR inputs, then run the predeclared isolated shadow trial.

Use the repository toolchain environment. No production gate or backend is run.
Formal timing requires an explicit integration-team idle-window identifier.
"""
import argparse
import datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = 'tools/research/055/Cargo.toml'
CASES = 'tests/perf/impl055/cases.tsv'
BASELINE = '7e4c4728c09fd07a55680844514e0f38bb29e708'
CRATES = ('zil-base', 'zil-ir', 'zil-syntax', 'zil-expand', 'zil-types', 'zil-analyze', 'zil-verify')
METHODS = ('flat', 'cut', 'block', 'production')
FIELDS = ('front_ns', 'extract_ns', 'plan_ns', 'solve_ns', 'check_ns', 'drop_ns', 'total_ns', 'logical_solve_work')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n')


def command(args, log=None):
    result = subprocess.run([str(x) for x in args], cwd=ROOT, text=True, capture_output=True, check=False)
    if log:
        Path(log).write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f'command failed ({result.returncode}): {args}\n{result.stdout}\n{result.stderr}')
    return result.stdout


def source_inputs():
    paths = {'Cargo.toml', 'Cargo.lock', '_zil_design.md', MANIFEST,
             'docs/research/055/CONTRACT.md', CASES}
    for crate in CRATES:
        paths.update(command(['git', 'ls-files', f'crates/{crate}']).splitlines())
    for p in (ROOT / 'tools/research/055').iterdir():
        if p.suffix in {'.rs', '.py', '.toml', '.lock'}:
            paths.add(str(p.relative_to(ROOT)))
    for line in (ROOT / CASES).read_text().splitlines():
        if line and not line.startswith('#'):
            paths.add(line.split('\t')[1])
    return sorted(paths)


def pack_inputs(folder):
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode='w') as archive:
        for path in sorted((folder / 'inputs').iterdir()):
            blob = path.read_bytes()
            info = tarfile.TarInfo(path.name)
            info.size = len(blob)
            info.mode = 0o644
            info.mtime = 0
            archive.addfile(info, io.BytesIO(blob))
    (folder / 'frozen-inputs.tar.gz').write_bytes(gzip.compress(raw.getvalue(), mtime=0))


def freeze(folder):
    paths = source_inputs()
    command(['git', 'ls-files', '--error-unmatch', '--', *paths])
    command(['git', 'diff', '--exit-code', 'HEAD', '--', *paths])
    folder.mkdir(parents=True, exist_ok=False)
    command(['cargo', 'test', '--manifest-path', MANIFEST, '--offline', '--locked'], folder / 'tests.log')
    command(['cargo', 'clippy', '--manifest-path', MANIFEST, '--offline', '--locked', '--all-targets', '--', '-D', 'warnings'], folder / 'clippy.log')
    command(['cargo', 'build', '--manifest-path', MANIFEST, '--release', '--offline', '--locked'], folder / 'build.log')
    metadata = json.loads(command(['cargo', 'metadata', '--manifest-path', MANIFEST,
                                   '--offline', '--no-deps', '--format-version', '1']))
    binary = Path(metadata['target_directory']) / 'release/cut055'
    # Freeze an executable copy so later builds cannot silently replace it.
    frozen_binary = folder / 'cut055'
    frozen_binary.write_bytes(binary.read_bytes())
    frozen_binary.chmod(0o755)
    census = command([frozen_binary, 'freeze', CASES, folder / 'inputs'], folder / 'freeze.log')
    (folder / 'census.jsonl').write_text(census)
    records = [json.loads(line) for line in census.splitlines()]
    if not records or not all(r['production_verified'] and r['full_shadow_equal'] for r in records):
        raise RuntimeError('incomplete functional freeze')
    pack_inputs(folder)
    manifest = {
        'version': 1, 'status': 'FrozenBeforeTiming', 'baseline_commit': BASELINE,
        'source_commit': command(['git', 'rev-parse', 'HEAD']).strip(),
        'baseline_semantics': 'draft10 TypedIR schema7; descriptor/borrowed-closure may-liveness',
        'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'source_sha256': {p: sha(ROOT / p) for p in paths},
        'frozen_output_sha256': {p.name: sha(p) for p in sorted((folder / 'inputs').iterdir())},
        'archive_sha256': sha(folder / 'frozen-inputs.tar.gz'),
        'binary_sha256': sha(frozen_binary),
        'rustc': command(['rustc', '-vV']).strip(),
        'cargo': command(['cargo', '--version']).strip(),
        'build_environment': {key: os.environ.get(key) for key in (
            'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_PROFILE_DEV_DEBUG',
            'CARGO_PROFILE_TEST_DEBUG', 'CARGO_PROFILE_RELEASE_DEBUG',
            'CARGO_INCREMENTAL', 'RUSTFLAGS', 'RUSTDOCFLAGS')},
        'recipe': {'cases': len(records), 'rounds': 6, 'samples_per_round': 7,
                   'methods': METHODS, 'warmup_per_method_case': 1,
                   'ordering': '(offset + round + sample) modulo 4',
                   'timed_check': 'raw extraction audit plus full output against frozen independent oracle',
                   'oracle_construction_timed': False,
                   'production_is_equivalent_baseline': False},
        'limits': {'nodes': 200000, 'slots': 8192, 'edges': 2000000, 'stage_work': 100000000, 'fill': 64,
                   'freeze_stage_calls_per_function': 19, 'shadow_sample_stage_calls_per_function': 6,
                   'unit_scope': 'research per-function charged element/word/byte work; frontend/production use their own budgets'},
        'tests_log_sha256': sha(folder / 'tests.log'),
        'clippy_log_sha256': sha(folder / 'clippy.log'),
        'census_sha256': sha(folder / 'census.jsonl'),
    }
    save(folder / 'input-manifest.json', manifest)
    print(json.dumps({'status': 'FrozenBeforeTiming', 'folder': str(folder), 'cases': len(records),
                      'source_commit': manifest['source_commit'], 'input_manifest_sha256': sha(folder / 'input-manifest.json')}))


def check_frozen(folder, manifest):
    if (folder / 'INVALIDATED.json').exists():
        raise RuntimeError('this freeze was invalidated before formal timing')
    if manifest['status'] != 'FrozenBeforeTiming':
        raise RuntimeError('incorrect freeze status')
    if sorted(manifest['source_sha256']) != source_inputs():
        raise RuntimeError('source input set changed; create a new freeze')
    for path, digest in manifest['source_sha256'].items():
        if sha(ROOT / path) != digest:
            raise RuntimeError(f'source changed after freeze: {path}')
    for name, digest in manifest['frozen_output_sha256'].items():
        if sha(folder / 'inputs' / name) != digest:
            raise RuntimeError(f'frozen oracle input changed: {name}')
    if sha(folder / 'cut055') != manifest['binary_sha256']:
        raise RuntimeError('frozen binary changed')
    if sha(folder / 'frozen-inputs.tar.gz') != manifest['archive_sha256']:
        raise RuntimeError('frozen archive changed')


def quantile(values, fraction):
    ordered = sorted(values)
    index = (len(ordered) - 1) * fraction
    lo = int(index)
    hi = min(lo + 1, len(ordered) - 1)
    return ordered[lo] * (hi - index) + ordered[hi] * (index - lo) if lo != hi else ordered[lo]


def stats(values):
    return {'n': len(values), 'median': statistics.median(values),
            'p10': quantile(values, .1), 'p90': quantile(values, .9),
            'q25': quantile(values, .25), 'q75': quantile(values, .75),
            'iqr': quantile(values, .75) - quantile(values, .25),
            'min': min(values), 'max': max(values)}


def summarize(rows, manifest):
    expected = manifest['recipe']['cases'] * 6 * 7 * 4
    if len(rows) != expected:
        raise RuntimeError(f'missing samples: got {len(rows)}, expected {expected}')
    keyed = {(r['case'], r['method'], r['round'], r['sample']): r for r in rows}
    if len(keyed) != expected:
        raise RuntimeError('duplicate samples')
    cases = sorted({r['case'] for r in rows})
    result = {}
    for case in cases:
        methods = {method: {field: stats([keyed[case, method, r, s][field] for r in range(6) for s in range(7)])
                            for field in FIELDS} for method in METHODS}
        ratios = {}
        for reference in ('flat', 'block'):
            ratios[f'cut_over_{reference}_total'] = stats([
                keyed[case, 'cut', r, s]['total_ns'] / keyed[case, reference, r, s]['total_ns']
                for r in range(6) for s in range(7)])
            ratios[f'cut_over_{reference}_plan_solve'] = stats([
                (keyed[case, 'cut', r, s]['plan_ns'] + keyed[case, 'cut', r, s]['solve_ns']) /
                (keyed[case, reference, r, s]['plan_ns'] + keyed[case, reference, r, s]['solve_ns'])
                for r in range(6) for s in range(7)])
        result[case] = {'methods': methods, 'paired_ratios': ratios}
    return {'status': 'Pass', 'samples': len(rows), 'ratio_interpretation': 'cut/reference; below 1 is faster',
            'production_role': 'context only; includes full Analyze/Verify, not equivalent to the narrower shadow query',
            'cases': result}


def compiler_inventory():
    """Names only: procps fails in this container's process namespace."""
    names, unreadable, self_visible = [], 0, False
    try:
        entries = list(Path('/proc').iterdir())
    except OSError as error:
        return {'available': False, 'reason': str(error), 'compilers': []}
    for entry in entries:
        if not entry.name.isdigit():
            continue
        try:
            name = (entry / 'comm').read_text().strip()
        except OSError:
            unreadable += 1
            continue
        self_visible |= int(entry.name) == os.getpid()
        names.append(name)
    return {'available': True, 'mode': 'proc_comm_only', 'self_visible': self_visible,
            'entries': len(names), 'unreadable_or_exited': unreadable,
            'compilers': [p for p in names if p in {'rustc', 'cargo', 'clang', 'clang-23', 'cc1', 'ld', 'lld', 'rust-lld'}]}


def bench(folder, cpu, idle_window):
    manifest = json.loads((folder / 'input-manifest.json').read_text())
    check_frozen(folder, manifest)
    if (folder / 'bench-start.json').exists():
        raise RuntimeError('trial already started; preserve it and create a new folder/freeze')
    allowed = os.sched_getaffinity(0)
    cpu = min(allowed) if cpu is None else cpu
    if cpu not in allowed:
        raise RuntimeError(f'CPU {cpu} unavailable')
    inventory = compiler_inventory()
    if inventory['compilers']:
        raise RuntimeError('another compiler is active; retain the integration idle-window requirement')
    start = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'cpu': cpu,
             'allowed_cpus': sorted(allowed), 'idle_window': idle_window,
             'platform': platform.platform(), 'loadavg': os.getloadavg(),
             'process_inventory': inventory,
             'input_manifest_sha256': sha(folder / 'input-manifest.json')}
    save(folder / 'bench-start.json', start)
    try:
        os.sched_setaffinity(0, {cpu})
        raw = command([folder / 'cut055', 'bench', CASES, folder / 'inputs'], folder / 'bench.log')
    finally:
        os.sched_setaffinity(0, allowed)
    (folder / 'paired-raw.jsonl').write_text(raw)
    check_frozen(folder, manifest)
    summary = summarize([json.loads(line) for line in raw.splitlines()], manifest)
    summary.update(input_manifest_sha256=start['input_manifest_sha256'],
                   raw_sha256=sha(folder / 'paired-raw.jsonl'), cpu=cpu, idle_window=idle_window)
    save(folder / 'paired-summary.json', summary)
    print(json.dumps({'status': 'Pass', 'folder': str(folder), 'samples': summary['samples'], 'cpu': cpu}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['freeze', 'bench'])
    parser.add_argument('--folder', required=True)
    parser.add_argument('--cpu', type=int)
    parser.add_argument('--idle-window', help='integration-team approval/reference; required for timing')
    args = parser.parse_args()
    folder = (ROOT / args.folder).resolve()
    if not folder.is_relative_to(ROOT / 'out/runs/research/055'):
        parser.error('folder must be within out/runs/research/055')
    if args.operation == 'bench' and not args.idle_window:
        parser.error('formal timing requires a coordinated --idle-window identifier')
    if args.operation == 'freeze':
        freeze(folder)
    else:
        bench(folder, args.cpu, args.idle_window)


if __name__ == '__main__':
    main()

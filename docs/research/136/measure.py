#!/usr/bin/env python3
"""Paired full-process experiment; raw data is outside Git, not a release gate.

Build both exact revisions with the same locked toolchain and release profile.
Run when no other local build/test jobs are active. This measures new processes
with warm filesystem caches, not a cold machine or a persistent compiler session.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(n: int, width: int, cyclic: bool, reverse: bool = False) -> str:
    if n == 0:
        return 'fn main()->i32{return 0}\n'
    params = ','.join(f'a{i}:R' for i in range(width))
    rotated = ','.join(f'a{(i+1)%width}' for i in range(width))
    chunks = ['resource R{n:i32}\n']
    for i in (reversed(range(n)) if reverse else range(n)):
        if cyclic:
            body = f'if steps>0{{f{(i+1)%n}({rotated},steps-1)}}else{{a0.n+=1}}'
        elif i+1 < n:
            body = f'f{i+1}({rotated},steps)'
        else:
            body = 'a0.n+=1'
        chunks.append(f'fn f{i}({params},steps:i32){{{body}}}\n')
    chunks.append('fn make()->impl FnMut()->i32{')
    chunks.extend(f'let a{i}=R{{n:0}};' for i in range(width))
    names = ','.join(f'a{i}' for i in range(width))
    total = '+'.join(f'a{i}.n' for i in range(width))
    chunks.append(f'return move ()=>{{f0({names},17);return {total}}}}}')
    chunks.append('fn main()->i32{var f=make();return f()-1}\n')
    return ''.join(chunks)


def git(repo: str, *args: str) -> str:
    return subprocess.check_output(['git', '-C', repo, *args], text=True).strip()


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--base', required=True, type=Path)
    p.add_argument('--candidate', required=True, type=Path)
    p.add_argument('--base-repo', required=True)
    p.add_argument('--candidate-repo', required=True)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--base-built-commit', required=True)
    p.add_argument('--candidate-built-commit', required=True)
    p.add_argument('--blocks', type=int, default=16)
    p.add_argument('--timeout', type=float, default=20)
    p.add_argument('--clang', default='clang')
    p.add_argument('--preflight-only', action='store_true')
    a = p.parse_args()
    if a.blocks < 4 or a.timeout <= 0:
        p.error('Use at least four blocks and a positive timeout')
    binaries = {'A': a.base.resolve(strict=True), 'B': a.candidate.resolve(strict=True)}
    a.out.mkdir(parents=True, exist_ok=False)
    case_defs = [
        ('tiny', 0, 0, False, False),
        ('below-threshold', 15, 2, False, False),
        ('chain16-caller-first', 16, 2, False, False),
        ('chain64-caller-first', 64, 2, False, False),
        ('chain64-callee-first', 64, 2, False, True),
        ('chain24-wide8', 24, 8, False, False),
        ('cycle16-width4', 16, 4, True, False),
        ('cycle16-width8', 16, 8, True, False),
        ('cycle16-width16', 16, 16, True, False),
        ('cycle32-width8', 32, 8, True, False),
    ]
    inputs = {}
    for name, *args in case_defs:
        d = a.out / 'inputs' / name
        d.mkdir(parents=True)
        f = d / 'main.zil'
        f.write_text(source(*args), encoding='utf-8')
        inputs[name] = f.resolve()
    cells = [(name, stage) for name in inputs for stage in ['types', 'verify']]
    cells += [(name, opt) for name in ['tiny', 'chain24-wide8', 'cycle16-width8'] for opt in ['O0', 'O2']]
    affinity = sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else []
    if affinity:
        os.sched_setaffinity(0, {affinity[0]})
    identity = {}
    for label, repo, built in [('A', a.base_repo, a.base_built_commit), ('B', a.candidate_repo, a.candidate_built_commit)]:
        for item in ['crates', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']:
            if git(repo, 'rev-parse', 'HEAD:'+item) != git(repo, 'rev-parse', built+':'+item):
                raise SystemExit('Current production source differs from declared built revision')
        identity[label] = {
            'binary': str(binaries[label]), 'sha256': digest(binaries[label]),
            'commit': git(repo, 'rev-parse', 'HEAD'), 'built_commit': built,
            'tree': git(repo, 'rev-parse', 'HEAD^{tree}'),
            'production_tree': {d: git(repo, 'rev-parse', f'HEAD:{d}') for d in ['crates', 'spec', 'policy']},
            'dirty': git(repo, 'status', '--porcelain'),
        }
    plan = {
        'format': 'zil-136-process-pairs/1', 'mode': 'preflight' if a.preflight_only else 'measurement',
        'script_sha256': digest(Path(__file__)), 'identity': identity,
        'cases': case_defs, 'cells': cells, 'blocks': a.blocks,
        'order': 'ABBA on even blocks, BAAB on odd blocks; cells interleaved and reversed every block',
        'warmups': 'one AB per cell, retained and excluded from timing summaries',
        'timeout_seconds': a.timeout, 'pinned_cpu': affinity[:1],
        'os': platform.platform(), 'clang': subprocess.check_output([a.clang, '--version'], text=True),
        'inputs': {name: {'path': str(f), 'sha256': digest(f)} for name, f in inputs.items()},
        'scope': 'exploratory same-machine full-process cost; not product acceptance or independent review',
    }
    (a.out / 'plan.json').write_text(json.dumps(plan, indent=2))
    records = []
    raw = (a.out / 'raw.jsonl').open('w')
    serial = 0

    def run(label: str, name: str, stage: str, block: int, position: int) -> dict:
        nonlocal serial
        serial += 1
        run_dir = a.out / 'runs' / f'{serial:05d}-{label}'
        run_dir.mkdir(parents=True)
        argv = [str(binaries[label]), 'check' if stage in ['types', 'verify'] else 'build', str(inputs[name]), '--json']
        if stage in ['types', 'verify']:
            argv += ['--stop-after', stage]
        else:
            argv += ['--opt', stage, '--clang', a.clang, '-o', str(run_dir / 'program')]
        if block == -1:
            argv += ['--emit-dir', str(run_dir / 'phases')]
        start = time.perf_counter_ns()
        try:
            proc = subprocess.run(argv, capture_output=True, timeout=a.timeout)
            elapsed = time.perf_counter_ns() - start
            rc, stdout, stderr = proc.returncode, proc.stdout, proc.stderr
        except subprocess.TimeoutExpired as e:
            elapsed = time.perf_counter_ns() - start
            rc, stdout, stderr = 124, e.stdout or b'', e.stderr or b''
        (run_dir / 'stdout.json').write_bytes(stdout)
        (run_dir / 'stderr.txt').write_bytes(stderr)
        try:
            report = json.loads(stdout)
        except (ValueError, UnicodeDecodeError):
            report = {'status': 'InvalidReport'}
        native = None
        if stage in ['O0', 'O2'] and rc == 0 and report.get('status') == 'Success':
            try:
                q = subprocess.run([str(run_dir / 'program')], capture_output=True, timeout=a.timeout)
                native = {'exit': q.returncode, 'stdout_sha256': hashlib.sha256(q.stdout).hexdigest(), 'stderr_sha256': hashlib.sha256(q.stderr).hexdigest()}
                (run_dir / 'native.stdout').write_bytes(q.stdout)
                (run_dir / 'native.stderr').write_bytes(q.stderr)
            except subprocess.TimeoutExpired:
                native = {'exit': 124}
        row = {'serial': serial, 'label': label, 'case': name, 'stage': stage, 'block': block,
               'position': position, 'ns': elapsed, 'exit': rc, 'status': report.get('status'),
               'phase': report.get('phase'), 'diagnostics': report.get('diagnostics', []),
               'verification_runs': report.get('verification_runs'), 'native': native, 'argv': argv}
        records.append(row)
        raw.write(json.dumps(row) + '\n'); raw.flush()
        return row

    for name, stage in cells:
        for i, label in enumerate('AB'):
            run(label, name, stage, -1, i)
    bad_warmups = [r for r in records if r['exit'] != 0 or r['status'] != 'Success' or (r['native'] is not None and r['native'].get('exit') != 0)]
    # Never silently remove a slow or failing case. Preserve failed preflight and stop.
    if bad_warmups:
        (a.out / 'failure.json').write_text(json.dumps(bad_warmups, indent=2))
        raise SystemExit(f'{len(bad_warmups)} failed preflight observations; retained, no timing claims')
    if a.preflight_only:
        raw.close()
        print(json.dumps({'preflight_observations': len(records), 'success': True}))
        return
    for block in range(a.blocks):
        for name, stage in (cells if block % 2 == 0 else reversed(cells)):
            for i, label in enumerate('ABBA' if block % 2 == 0 else 'BAAB'):
                run(label, name, stage, block, i)
        print(f'block {block + 1}/{a.blocks}, observations={len(records)}', flush=True)
    raw.close()
    summaries = []
    rng = random.Random(136)
    for name, stage in cells:
        rows = [r for r in records if r['case'] == name and r['stage'] == stage and r['block'] >= 0]
        errors = [r['serial'] for r in rows if r['exit'] != 0 or r['status'] != 'Success' or (r['native'] is not None and r['native'].get('exit') != 0)]
        ratios = []
        for block in range(a.blocks):
            times = {k: [r['ns'] for r in rows if r['block'] == block and r['label'] == k] for k in 'AB'}
            ratios.append(statistics.fmean(times['B']) / statistics.fmean(times['A']))
        boots = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000))
        summaries.append({'case': name, 'stage': stage, 'blocks': a.blocks, 'errors': errors,
                          'A_median_ms': statistics.median(r['ns'] for r in rows if r['label'] == 'A') / 1e6,
                          'B_median_ms': statistics.median(r['ns'] for r in rows if r['label'] == 'B') / 1e6,
                          'paired_ratio_median': statistics.median(ratios),
                          'bootstrap95_ratio': [boots[49], boots[1949]], 'block_ratios': ratios})
    result = {'status': 'Pass' if not any(x['errors'] for x in summaries) else 'Fail',
              'observations': len(records), 'native_executions': sum(r['native'] is not None for r in records),
              'timing_scope': plan['scope'], 'cells': summaries}
    (a.out / 'summary.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()

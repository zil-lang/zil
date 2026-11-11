#!/usr/bin/env python3
"""Prepare only: freeze tools and generate unchanged two-case 100k runtime corpus."""
import argparse
from pathlib import Path
from common import *

def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--out', type=Path, required=True)
    for name in ['baseline', 'exporter', 'rustc', 'clang', 'lld', 'opt', 'bcanalyzer', 'dis']:
        q.add_argument('--' + name, type=Path, required=True)
    q.add_argument('--baseline-build-receipt', type=Path, required=True)
    q.add_argument('--exporter-build-receipt', type=Path, required=True)
    q.add_argument('--cpus', default='0,1')
    a = q.parse_args()
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    p = {'format': 'zil-thinlto-pure-runtime/1', 'status': 'prepared-not-run',
         'source_claims': {'A': MAIN, 'B': EXPORTER},
         'build_receipts': {arm: identity(path) for arm, path in [('A', a.baseline_build_receipt), ('B', a.exporter_build_receipt)]},
         'tools': {name: identity(getattr(a, name)) for name in ['baseline', 'exporter', 'rustc', 'clang', 'lld', 'opt', 'bcanalyzer', 'dis']},
         'cpus': [int(v) for v in a.cpus.split(',')], 'jobs': 2, 'cases_requested': [64, 256],
         'iterations': 100000, 'orders': ORDERS,
         'limits': {'B_over_A_max': 1.10, 'R_over_B_min': .90},
         'counts': {'cases': 2, 'clean_compilations': 6, 'native_formal': 36, 'native_warmups': 6, 'native_functional': 12, 'native_total': 54},
         'pipeline_A': 'main990 LLVM API O2 incremental-o2 after, no object cache',
         'pipeline_B': 'research O2 incremental-o2 after canonical owners; clang23 ThinLTO prelink O2; lld23 ThinLTO O2 after merge, no aliases/ICF',
         'scope': 'pure multi-file base fixtures only; no address/global-state correctness claim; no product adoption or compile-speed claim',
         'compile_timing': 'entire ZIL request including exporter/clang prelink/thinlink/LLVM backend/native link/verification; diagnostic cold, not compile performance gate',
         'runtime_timing': 'full native launch+one byte input through wait4; original period251 arithmetic oracle, no dropped samples',
         'scripts': scripts(), 'cases': {}}
    for n in [64, 256]:
        reference = a.out / 'reference' / f'n{n}'
        generate(reference, n, 0, functions=8, flavor='dynamic-body')
        p['cases'][f'n{n}/base'] = generate_runtime(a.out / f'n{n}/base', n, 0, reference)
    validate(p)
    write(a.out / 'protocol.json', p)
    print(sha(a.out / 'protocol.json'))

if __name__ == '__main__':
    main()

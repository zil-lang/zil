#!/usr/bin/env python3
"""Prepare unchanged two-case fixture; no compiler or native execution."""
import argparse
from pathlib import Path
from common import *


def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--out', type=Path, required=True)
    for name in ['baseline', 'candidate', 'rustc', 'clang']:
        q.add_argument('--' + name, type=Path, required=True)
    q.add_argument('--llvm-dso', type=Path, required=True)
    q.add_argument('--baseline-build-receipt', type=Path, required=True)
    q.add_argument('--candidate-build-receipt', type=Path, required=True)
    q.add_argument('--candidate-commit', required=True)
    q.add_argument('--candidate-tree', required=True)
    q.add_argument('--reference-protocol', type=Path, required=True)
    a = q.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    reference = identity(a.reference_protocol)
    if reference['sha256'] != REFERENCE_SHA:
        raise ValueError('original linear protocol hash mismatch')
    original = json.loads(Path(reference['path']).read_text())
    p = {'format': 'zil-full-producer-compile-screen/1', 'status': 'prepared-not-run',
         'sources': {'A': {'commit': BASELINE_COMMIT, 'tree': BASELINE_TREE},
                     'B': {'commit': a.candidate_commit, 'tree': a.candidate_tree}},
         'tools': {name: identity(getattr(a, name)) for name in ['baseline', 'candidate', 'rustc', 'clang']},
         'build_receipts': {'A': identity(a.baseline_build_receipt), 'B': identity(a.candidate_build_receipt)},
         'llvm_dso': dso_identity(a.llvm_dso),
         'dynamic_linker_environment': {'LD_LIBRARY_PATH': os.environ.get('LD_LIBRARY_PATH')},
         'llvm_versions': LLVM_VERSIONS,
         'reference_protocol': reference, 'scripts': scripts(), 'cpus': original['cpus'], 'jobs': 2,
         'cases': {}, 'iterations': 100000, 'orders': ORDERS, 'compile_screen_counts': COUNTS,
         'limits': {'B_over_A_max': 1.10, 'R_over_B_min': .90},
         'candidate_receipt_schema': 'zil-manual-prelink-preflight/1+unit/2+object/2',
         'pipeline_A': 'main990 llvm-api O2 incremental-o2 after8units2jobs no-object-cache',
         'pipeline_B': 'native canonical default<O2>; actual Required empty emits verified canonical object directly; nonempty imports then full thinlto<O2>; same target/O2/8units2jobs; ordinary Clang link',
         'identity_validation_policy': 'symmetric-process-external-prevalidation; complete-native-compiler-request-timed',
         'scope': 'two pure base runtime cases; no production, address/global, many-edit or concurrency claim'}
    for n in [64, 256]:
        reference_source = out / 'reference' / f'n{n}'
        generate(reference_source, n, 0, functions=8, flavor='dynamic-body')
        p['cases'][f'n{n}/base'] = generate_runtime(out / f'n{n}/base', n, 0, reference_source)
    validate(p)
    write(out / 'protocol.json', p)
    print(sha(out / 'protocol.json'))


if __name__ == '__main__':
    main()

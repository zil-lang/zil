#!/usr/bin/env python3
"""Future preparation only; freeze runtime sources, tools and balanced schedule."""
import argparse
import datetime
import json
from pathlib import Path
from fixture import sha
from runtime_fixture import generate_runtime

ORDERS = ['ABR', 'BRA', 'RAB', 'ARB', 'RBA', 'BAR']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--repo', type=Path, default=Path('/workspace/scratch/abf690ce880c/zil'))
    parser.add_argument('--reference-protocol', type=Path, default=Path('/workspace/scratch/abf690ce880c/out/multi-edit/final-inc-after-v2/protocol.json'))
    for name in ['baseline', 'candidate', 'rustc', 'clang']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--baseline-source', required=True)
    parser.add_argument('--candidate-source', required=True)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    reference = json.loads(args.reference_protocol.read_text())
    if reference['files'] != [64, 256] or reference['flavor'] != 'dynamic-body':
        raise ValueError('runtime gate must use the original dynamic 64/256 corpus')
    protocol = {'format': 'zil-bounded-imports-runtime-quality/1', 'status': 'prepared-not-run',
                'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'repo': str(args.repo.resolve()), 'policy': 'incremental-o2-after',
                'reference_protocol': {'path': str(args.reference_protocol.resolve()), 'sha256': sha(args.reference_protocol)},
                'source_claims': {'A': args.baseline_source, 'B': args.candidate_source},
                'clang': {'path': str(args.clang.resolve(strict=True)), 'sha256': sha(args.clang)},
                'binaries': {arm: {'path': str(path.resolve(strict=True)), 'sha256': sha(path)}
                             for arm, path in [('A', args.baseline), ('B', args.candidate), ('R', args.rustc)]},
                'fixed_toolchains_required': {'llvm': '23.1.4', 'rust': '1.99.1'},
                'cpus': [0, 1], 'frequency': {'policy': 'observed-uncontrolled', 'fixed_khz': None},
                'optimization': 'O2', 'cgu': 8, 'jobs': 2,
                'iterations': 100000, 'seed_period': 251, 'acc_modulus': 1000003,
                'stdin_timing_hex': '41', 'extra_functional_stdin_hex': ['00', 'ff'],
                'warmup_order': 'ABR', 'formal_orders': ORDERS,
                'per_arm_per_case': {'warmups': 1, 'formal': 6, 'extra_functional': 2},
                'compile_timeout_seconds': 180, 'native_timeout_seconds': 120,
                'timing': 'full native process launch, one-byte stdin write through wait4; CPU user+system and ru_maxrss saved; stdout/stderr checked after clock',
                'gates': {'per_case_B_over_A_mean_wall_max': 1.10,
                          'per_case_Rust_over_B_mean_wall_min': .90,
                          'mean': 'arithmetic mean of all SIX formal samples, including failures/long tails; invalid sample fails case, never dropped',
                          'all_eight_cases_must_pass': True, 'geomeans_are_additional_only': True},
                'artifact_policy': 'clean no-object-cache full-O2 compile once per case/arm; A/B code difference allowed and classified; all independently predicted outputs required',
                'budget_policy': 'fixed existing cost32/source boundaries; do not tune for observed runtime; current independent Verify and machine preflight remain mandatory',
                'counts': {'cases': 8, 'clean_compilations': 24, 'native_formal': 144,
                           'native_warmup': 24, 'native_extra_functional': 48, 'native_total': 216},
                'tools': {name: sha(Path(__file__).with_name(name)) for name in
                          ['fixture.py', 'screen.py', 'runtime_fixture.py', 'prepare_runtime.py', 'runtime_gate.py']},
                'cases': {}}
    for n in [64, 256]:
        for k in [0, 1, 16, 64]:
            snapshot = 'base' if k == 0 else f'edit-{k}'
            key = f'n{n}/{snapshot}'
            ref = args.reference_protocol.parent / key
            item = generate_runtime(args.out / key, n, k, ref)
            protocol['cases'][key] = item
    (args.out / 'protocol.json').write_text(json.dumps(protocol, indent=2) + '\n')
    print(json.dumps({'status': 'prepared-not-run', 'protocol': str(args.out / 'protocol.json'),
                      'sha256': sha(args.out / 'protocol.json'), 'counts': protocol['counts']}, indent=2))


if __name__ == '__main__':
    main()

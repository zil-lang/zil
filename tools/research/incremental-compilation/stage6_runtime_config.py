#!/usr/bin/env python3
"""Bind the fixed nine runtime arms to a frozen compiler pair and stock baseline."""
import argparse
import copy
import json
from pathlib import Path

from stage6_toolchain_runtime import ARM_OPTIONS, identity, load_config


def checked(spec):
    actual = identity(spec['path'])
    if actual['sha256'] != spec['sha256']:
        raise ValueError('producer identity mismatch: ' + spec['path'])
    return actual


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pair', type=Path, required=True)
    parser.add_argument('--baseline-source-receipt', type=Path, required=True)
    parser.add_argument('--baseline-llvm-receipt', type=Path, required=True)
    for name in ['rustc', 'clang', 'nm', 'size', 'out']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error('refusing to overwrite a configuration')
    pair = json.loads(args.pair.read_text())
    baseline = json.loads(args.baseline_source_receipt.read_text())
    stock = json.loads(args.baseline_llvm_receipt.read_text())
    if pair['status'] != 'Pass' or stock['status'] != 'Pass':
        raise ValueError('pair and stock toolchain must have passing receipts')
    if [arm['name'] for arm in pair['arms']] != ['original', 'candidate']:
        raise ValueError('expected the explicit original/candidate producer pair')
    original, candidate = pair['arms']
    if (original['production_revision'] != candidate['production_revision'] or
            original['original_files_manifest_sha256'] != candidate['original_files_manifest_sha256']):
        raise ValueError('producer pair must share source revision and original inventory')
    namespace = 'research-llvm-stage6'
    for arm in pair['arms']:
        expected_tag = '|' + namespace + '=' + arm['toolchain_arm'] + '|dso-sha256=' + arm['llvm']['library']['sha256']
        if arm['pipeline_identity_tag'] != expected_tag:
            raise ValueError('producer pair must use its explicit stage6 pipeline identity')
        source = json.loads(Path(checked(arm['source_receipt'])['path']).read_text())
        if (source['revision'] != arm['production_revision'] or
                source['arm'] != arm['toolchain_arm'] or
                source['original_files_manifest_sha256'] != arm['original_files_manifest_sha256'] or
                source['pipeline_identity_tag'] != arm['pipeline_identity_tag']):
            raise ValueError('pair metadata does not match its actual source receipt')
    stock_library = dict(stock['library'])
    if not Path(stock_library['path']).is_absolute():
        stock_library['path'] = str(args.baseline_llvm_receipt.parent / stock_library['path'])
    baseline_arm = {
        'language': 'zil',
        'compiler': checked({'path': baseline['binary'], 'sha256': baseline['sha256']}),
        'source_receipt': identity(args.baseline_source_receipt),
        'env': {'LD_LIBRARY_PATH': str(Path(stock_library['path']).resolve().parent)},
        'llvm': {'library': checked(stock_library), 'receipt': identity(args.baseline_llvm_receipt)},
        'expected_toolchain_arm': None,
    }
    roles = {'baseline': baseline_arm}
    for role, arm in [('original', original), ('entry', candidate)]:
        roles[role] = {
            'language': 'zil', 'compiler': checked(arm['compiler']),
            'source_receipt': checked(arm['source_receipt']), 'env': arm['env'],
            'llvm': {key: checked(arm['llvm'][key]) for key in ['library', 'receipt']},
            'expected_toolchain_arm': arm['toolchain_arm'],
        }
    config = {
        'schema': 1, 'candidate_variant': candidate['toolchain_arm'],
        'original_variant': original['toolchain_arm'],
        'pipeline_identity_namespace': namespace,
        'baseline_revision': baseline['commit'], 'candidate_revision': candidate['production_revision'],
        'producer_pair_receipt': identity(args.pair), 'producer': identity(Path(__file__)),
        'arms': [], 'tools': {name: identity(getattr(args, name)) for name in ['clang', 'nm', 'size']},
    }
    for name, options in ARM_OPTIONS.items():
        arm = ({'language': 'rust', 'compiler': identity(args.rustc), 'env': {}} if name == 'rust'
               else copy.deepcopy(roles[name.split('_', 1)[0]]))
        arm.update(name=name, options=options)
        config['arms'].append(arm)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('x') as stream:
        stream.write(json.dumps(config, indent=2) + '\n')
    # Keep a rejected configuration for diagnosis rather than silently replacing it.
    arms, _ = load_config(args.out.resolve(), False)
    print(json.dumps({'status': 'Pass', 'configuration': identity(args.out), 'arms': len(arms),
                      'scope': 'Exact identities only; preparation and native controls remain required.'}))


if __name__ == '__main__':
    main()

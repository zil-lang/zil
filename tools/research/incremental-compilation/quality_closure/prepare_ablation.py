#!/usr/bin/env python3
"""Prepare controlled IR ablations of actual captured units, not production code."""
import argparse
import hashlib
import json
from pathlib import Path
import re


def identity(path):
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def functions(text):
    result = {}
    for match in re.finditer(r'^define [^\n]*@([^ (]+)\([^\n]*\) [^\n]*\{\n.*?^}\n', text, re.M | re.S):
        result[match.group(1)] = match.group(0)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', action='append', choices=['8192-arithmetic', '2048-branch', '512-body'])
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    receipt = dict(scope=__doc__, producer=identity(Path(__file__)),
                   transformed_validation='LLVM parse/verify required after transformation; original structured preflight provenance is retained, not claimed for the new IR.', cases=[])
    for case in args.case or ['8192-arithmetic', '2048-branch']:
        size = int(case.split('-', 1)[0])
        directory = args.out / case
        directory.mkdir()
        originals = sorted((args.capture / case / 'units').glob('*.before.ll'))
        parsed = [(path, path.read_text()) for path in originals]
        owned = {}
        root_path = None
        for path, text in parsed:
            for name, body in functions(text).items():
                if 'available_externally ' not in body.splitlines()[0]:
                    assert name not in owned, ('duplicate canonical owner', name)
                    owned[name] = body
                    if name == f'zil.fn.{size}':
                        root_path = path
        assert root_path is not None
        root = root_path.read_text()
        pre_functions = functions(root)
        helpers = {f'zil.fn.{index}': owned[f'zil.fn.{index}'] for index in range(size)}
        # Narrow ablation input contract: helpers are plain scalar functions.
        for name, body in helpers.items():
            assert body.startswith(f'define i32 @{name}(i32 %arg.0) {{\n'), name
            assert not set(re.findall(r'@([^ (]+)', body)) - {name, 'llvm.trap',
                'llvm.sadd.with.overflow.i32', 'llvm.smul.with.overflow.i32'}, name
        missing = sorted(set(helpers) - set(pre_functions), key=lambda name: int(name.rsplit('.', 1)[1]))
        missing_set, removed = set(missing), set()
        def remove_import_declaration(match):
            name = match.group(1)
            if name not in missing_set:
                return match.group(0)
            assert name not in removed
            removed.add(name)
            return ''
        full = re.sub(r'^declare [^\n]*@([^ (]+)\([^\n]*\)[^\n]*\n',
                      remove_import_declaration, root, flags=re.M)
        assert removed == missing_set, 'missing helper is not an existing declaration'
        full += '\n'.join(helpers[name].replace('define ', 'define available_externally ', 1) for name in missing)
        def cloned(text):
            imported = {name: body for name, body in functions(text).items()
                        if 'available_externally ' in body.splitlines()[0]}
            names = {name: 'zil.research.import.' + name for name in imported}
            assert set(imported) <= set(helpers)
            output = re.sub(r'^define available_externally i32 @([^ (]+)\(',
                lambda match: 'define internal i32 @' + names[match.group(1)] + '(', text, flags=re.M)
            # Canonical addresses stay external and resolve to the real owner.
            output += '\n'.join(f'declare i32 @{name}(i32)' for name in imported) + '\n'
            direct = re.compile(r'(\b(?:call|invoke)\b[^@\n]*@)([^ (]+)(\()')
            output = direct.sub(lambda match: match.group(1) + names.get(match.group(2), match.group(2)) + match.group(3), output)
            return output, names
        variants = {'bounded-ae': root, 'full-ae': full}
        variants['bounded-internal'], bounded_names = cloned(root)
        variants['full-internal'], full_names = cloned(full)
        report = dict(case=case, original_root=identity(root_path), original_units=[identity(path) for path in originals],
                      source_helper_count=size, missing_helpers=len(missing),
                      bounded_clones=len(bounded_names), full_clones=len(full_names), variants={})
        for variant, text in variants.items():
            path = directory / (variant + '.ll')
            path.write_text(text)
            report['variants'][variant] = identity(path)
        receipt['cases'].append(report)
    (args.out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()

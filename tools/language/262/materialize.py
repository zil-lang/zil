#!/usr/bin/env python3
"""Copy pinned source bytes into one explicit filesystem/in-memory input graph."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / 'tests/impl262'


def prepare(name, variant, output):
    oracle = json.loads((FIXTURES / 'ORACLE.json').read_text())
    matches = [case for group in ['positive', 'negative'] for case in oracle[group]
               if case.get('name', case.get('entry')) == name]
    budget_path = FIXTURES / 'BUDGET-SOURCES.json'
    if budget_path.is_file():
        matches += [case for case in json.loads(budget_path.read_text())['cases']
                    if case['name'] == name]
    assert len(matches) == 1, name
    case = matches[0]
    assert variant in ['generated', 'handwritten']
    modules = case['handwritten_modules' if variant == 'handwritten' else
                   'generated_modules' if 'generated_modules' in case else 'modules']
    # Match ordinary filesystem discovery's source-order breadth-first module
    # numbering, rather than relying on the oracle's sorted graph inventory.
    by_name = {module['logical']: module for module in modules}
    order = ['<root>']
    for logical in order:
        text = (FIXTURES / by_name[logical]['source']).read_text()
        for dependency in re.findall(r'^(?:import|syntax use) ([A-Za-z0-9_.]+)(?: as \w+)?', text, re.M):
            if dependency in ['std.meta', 'std.syntax']:
                continue
            assert dependency in by_name, dependency
            if dependency not in order:
                order.append(dependency)
    assert set(order) == set(by_name)
    modules = [by_name[name] for name in order]
    output.mkdir(parents=True, exist_ok=False)
    inputs = output / 'inputs'
    inputs.mkdir()
    copies = []
    for index, module in enumerate(modules):
        source = FIXTURES / module['source']
        assert hashlib.sha256(source.read_bytes()).hexdigest() == module['sha256']
        destination = inputs / (case['entry'] if index == 0 else
                                module['logical'].replace('.', '/') + '.zil')
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        copies.append(dict(file_id=index, module_id=index, logical=module['logical'],
                           path=str(destination.resolve()), original=str(source.resolve()),
                           source_sha256=module['sha256']))
    lock = case.get('syntax_lock') if variant == 'generated' else None
    if lock:
        shutil.copy2(FIXTURES / 'locks' / lock, inputs / 'syntax.lock')
    request = dict(case=name, variant=variant, entry=copies[0]['path'],
                   syntax_lock=str((inputs / 'syntax.lock').resolve()) if lock else None,
                   syntax_lock_sha256=hashlib.sha256((inputs / 'syntax.lock').read_bytes()).hexdigest() if lock else None,
                   modules=copies, expected=case.get('normal'),
                   same_bytes_for_filesystem_and_in_memory=True)
    (output / 'project-input.json').write_text(json.dumps(request, indent=2) + '\n')
    # A bounded tab-separated protocol avoids adding a Rust JSON dependency.
    print(request['entry'])
    print(request['syntax_lock'] or '-')
    for module in copies:
        print(str(module['module_id']) + '\t' + module['logical'] + '\t' + module['path'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', required=True)
    parser.add_argument('--variant', choices=['generated', 'handwritten'], required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.case, args.variant, args.out)

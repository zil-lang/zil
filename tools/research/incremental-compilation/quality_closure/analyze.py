#!/usr/bin/env python3
"""Describe captured actual API modules, without invoking an optimizer."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re


def parse(path):
    functions, declarations = {}, []
    current = None
    data = path.read_text()
    for line in data.splitlines():
        if line.startswith('define '):
            name = re.search(r'@([^ (]+)\(', line).group(1)
            current = dict(header=line, instructions=0, blocks=0, calls=Counter(),
                           srem=0, checked_add=0, available_externally='available_externally ' in line)
            functions[name] = current
        elif line.startswith('declare '):
            declarations.append(re.search(r'@([^ (]+)\(', line).group(1))
        elif line == '}':
            current = None
        elif current is not None:
            if re.match(r'^[^ ;][^:]*:', line):
                current['blocks'] += 1
            elif line.startswith('  ') and line.strip() and not line.lstrip().startswith(';'):
                current['instructions'] += 1
                current['srem'] += bool(re.search(r'\bsrem\b', line))
                current['checked_add'] += '@llvm.sadd.with.overflow.' in line
                if re.search(r'\b(?:call|invoke)\b', line):
                    match = re.search(r'@([^ (]+)\(', line)
                    if match:
                        current['calls'][match.group(1)] += 1
    return dict(path=str(path), sha256=hashlib.sha256(data.encode()).hexdigest(),
                functions=functions, declarations=declarations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    result = dict(scope=__doc__, cases=[])
    for case, size in [('8192-arithmetic', 8192), ('2048-branch', 2048)]:
        report = dict(case=case, units=[])
        for before in sorted((args.capture / case / 'units').glob('*.before.ll')):
            after = before.with_name(before.name.replace('.before.', '.after.'))
            pre, post = parse(before), parse(after)
            helpers = {name: function for name, function in pre['functions'].items()
                       if name.startswith('zil.fn.') and int(name.rsplit('.', 1)[1]) < size}
            available = {name: function for name, function in helpers.items() if function['available_externally']}
            row = dict(before={key: pre[key] for key in ['path', 'sha256']},
                       after={key: post[key] for key in ['path', 'sha256']},
                       owned_helpers=len(helpers) - len(available), imported_helpers=len(available),
                       imported_expanded_cost=sum(f['instructions'] + f['blocks'] for f in available.values()),
                       imported_cost_histogram=dict(Counter(f['instructions'] + f['blocks'] for f in available.values())),
                       declared_helpers=[name for name in pre['declarations']
                                         if name.startswith('zil.fn.') and int(name.rsplit('.', 1)[1]) < size],
                       roots={})
            for name in [f'zil.fn.{size}', f'zil.fn.{size+3}', 'main']:
                if name in pre['functions'] or name in post['functions']:
                    row['roots'][name] = dict(before=pre['functions'].get(name), after=post['functions'].get(name))
            report['units'].append(row)
        result['cases'].append(report)
    args.out.write_text(json.dumps(result, indent=2) + '\n')
    for case in result['cases']:
        print(case['case'])
        for unit in case['units']:
            if unit['roots']:
                print(Path(unit['before']['path']).name, 'owned', unit['owned_helpers'], 'imports',
                      unit['imported_helpers'], 'cost', unit['imported_expanded_cost'],
                      'declared', len(unit['declared_helpers']))
                for name, states in unit['roots'].items():
                    for phase, function in states.items():
                        if function:
                            print(name, phase, 'inst', function['instructions'], 'blocks', function['blocks'],
                                  'srem', function['srem'], 'checked_add', function['checked_add'],
                                  'calls', sum(function['calls'].values()))


if __name__ == '__main__':
    main()

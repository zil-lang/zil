#!/usr/bin/env python3
"""Bind the unified ClosedBody gates to their actual source and raw logs.

Read-only aggregation: never runs compilers or treats an ignored test as a pass.
Performance windows and historical 98fa evidence remain separate inputs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re


COUNTS = {'native86': 86, 'backend': 269, 'driver': 211,
          'scalar-source': 9, 'retained-source-executed-02': 1}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def collect(directory, source):
    gates = []
    for name in [*COUNTS, 'clippy', 'fmt', 'release']:
        receipt = directory / (name + '.json')
        log = directory / (name + '.log')
        record = json.loads(receipt.read_text())
        if record.get('source') != source or record.get('exit') != 0:
            raise ValueError(name + ': source or exit mismatch')
        if not record.get('command') or digest(log) != record.get('log_sha256'):
            raise ValueError(name + ': missing command or raw log mismatch')
        results = re.findall(
            r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',
            log.read_text(), re.MULTILINE)
        if name in COUNTS and results != [(str(COUNTS[name]), '0', '0')]:
            raise ValueError(name + ': nonzero exact execution required')
        gates.append({'gate': name, 'source': source, 'command': record['command'],
                      'receipt': str(receipt), 'receipt_sha256': digest(receipt),
                      'log': str(log), 'log_sha256': digest(log),
                      'passed': COUNTS.get(name), 'exit': 0})
    return gates


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if not re.fullmatch('[0-9a-f]{40}', args.source):
        raise ValueError('exact source SHA required')
    gates = collect(args.directory, args.source)
    release = json.loads((args.directory / 'release.json').read_text())
    binary_sha256 = digest(args.binary)
    if release.get('binary_sha256') != binary_sha256:
        raise ValueError('release binary changed')
    result = {'schema': 'closed-body-unified-gates-v1', 'source': args.source,
              'binary_sha256': binary_sha256, 'gates': gates,
              'scope': 'Executed functional/strict/release gates only; no independent '
                       'review, timing, universal Rust lead or main merge implied.'}
    with args.output.open('x') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    main()

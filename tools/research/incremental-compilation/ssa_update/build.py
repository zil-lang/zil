#!/usr/bin/env python3
"""Reuse the reviewed single-object builder with one explicit additional arm name."""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['llvm-prefix', 'headers-prefix', 'sources', 'out']:
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    if not args.execute:
        parser.error('--execute requires an authorized functional window')
    original = Path(__file__).resolve().parent.parent / 'jump_threading_plugin/build_native_toolchain.py'
    source = original.read_text()
    old = "choices=['original', 'entry']"
    assert source.count(old) == 1
    derived = args.out.parent / (args.out.name + '-builder.py')
    assert not derived.exists()
    derived.parent.mkdir(parents=True, exist_ok=True)
    derived.write_text(source.replace(old, "choices=['original', 'entry', 'bulk']"))
    receipt = {'scope': __doc__, 'parent_builder': str(original),
               'parent_builder_sha256': hashlib.sha256(source.encode()).hexdigest(),
               'derived_builder_sha256': hashlib.sha256(derived.read_bytes()).hexdigest(),
               'only_text_change': {'before': old, 'after': "choices=['original', 'entry', 'bulk']"}}
    command = [sys.executable, str(derived), '--arm', 'bulk', '--jobs', '1', '--execute']
    for name in ['llvm_prefix', 'headers_prefix', 'sources', 'out']:
        command += ['--' + name.replace('_', '-'), str(getattr(args, name))]
    receipt['command'] = command
    receipt_path = derived.with_suffix('.json')
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    result = subprocess.run(command)
    receipt['exit'] = result.returncode
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    raise SystemExit(result.returncode)

if __name__ == '__main__':
    main()

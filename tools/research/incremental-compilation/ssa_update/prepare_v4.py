#!/usr/bin/env python3
"""Prepare both v4 translation units from vendored pinned sources in one command."""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    here = Path(__file__).resolve().parent
    commands = [
        [sys.executable, '-B', str(here / 'prepare.py'), '--out', str(args.out / 'v1')],
        [sys.executable, '-B', str(here / 'prepare_grouped_bulk.py'),
         '--jump-threading-source', str(args.out / 'v1'), '--out', str(args.out / 'v4')],
    ]
    for command in commands:
        subprocess.run(command, check=True)
    manifest = args.out / 'v4/source-manifest.json'
    (args.out / 'prepare-receipt.json').write_text(json.dumps({
        'status': 'Pass', 'scope': __doc__, 'commands': commands,
        'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'source_manifest': str(manifest.resolve()),
        'source_manifest_sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(),
    }, indent=2) + '\n')


if __name__ == '__main__':
    main()

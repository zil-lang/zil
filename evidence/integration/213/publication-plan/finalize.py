#!/usr/bin/env python3
"""Fill the exact reviewed source and bundle checksum; does not write any remote."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from publish import SHA, digest, require, Refusal, write_json

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--template', required=True)
parser.add_argument('--bundle', required=True)
parser.add_argument('--source-sha', required=True)
parser.add_argument('--out', required=True)
args = parser.parse_args()
try:
    require(SHA.fullmatch(args.source_sha), 'source SHA must be exactly 40 lowercase hexadecimal characters')
    manifest = json.loads(Path(args.template).read_text(encoding='utf-8'))
    bundle = Path(args.bundle).resolve()
    result = subprocess.run(['git', 'bundle', 'list-heads', str(bundle)], text=True, capture_output=True, check=True)
    advertised = f"{args.source_sha} {manifest['source']['bundle_ref']}"
    require(advertised in result.stdout.splitlines(), 'bundle does not advertise the supplied exact source')
    manifest['source']['sha'] = args.source_sha
    manifest['source']['bundle_sha256'] = digest(bundle)
    write_json(args.out, manifest)
    print(json.dumps({'manifest': str(Path(args.out).resolve()), 'source_sha': args.source_sha,
                      'bundle_sha256': manifest['source']['bundle_sha256'], 'bundle_bytes': bundle.stat().st_size}, indent=2))
except (Refusal, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
    print('FINALIZATION REFUSED: ' + str(error), file=sys.stderr)
    sys.exit(1)

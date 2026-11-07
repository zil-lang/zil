#!/usr/bin/env python3
"""Check current navigation and the exact provenance of relocated documents."""
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


def check(root=ROOT):
    errors = []
    index = json.loads((root / 'validation/documentation-provenance.json').read_text())
    source = index['source_commit']
    if not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', source):
        raise ValueError('source_commit must be an exact Git commit')
    archive = subprocess.run(['git', 'archive', source, 'docs', 'GITHUB-SYNC.md',
                              'SOURCE-SNAPSHOT-20261026.md'], cwd=root, check=True,
                             stdout=subprocess.PIPE).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as saved:
        original = {f.name: saved.extractfile(f).read() for f in saved.getmembers() if f.isfile()}
    entries = index['files']
    names = [f['source_path'] for f in entries]
    if len(names) != len(set(names)) or set(names) != set(original):
        errors.append('Provenance must cover the original document set exactly once')
    targets = [f['destination'] for f in entries if f['destination']]
    if len(targets) != len(set(targets)):
        errors.append('Two source documents claim the same destination')
    for item in entries:
        name, target = item['source_path'], item['destination']
        if item['handling'] not in {'history-only', 'frozen-bytes', 'current-document'} or ((target is None) != (item['handling'] == 'history-only')):
            errors.append(f'Invalid provenance handling: {name}')
        data = original.get(name)
        if data is None or hashlib.sha256(data).hexdigest() != item['sha256']:
            errors.append(f'Wrong historical document digest: {name}')
        if target:
            path = (root / target).resolve()
            if not path.is_relative_to(root.resolve()) or not path.is_file():
                errors.append(f'Missing/escaping document destination: {target}')
            elif item['handling'] == 'frozen-bytes' and path.read_bytes() != data:
                errors.append(f'Frozen document changed: {target}')
        if name != target and (root / name).is_file():
            errors.append(f'Old document alias remains: {name}')
    # The current handoff selector is editable; the historical policy and records are not.
    item = next(f for f in entries if f['source_path'] == 'docs/validation/handoff-migration-326.json')
    before, after = json.loads(original[item['source_path']]), json.loads((root / item['destination']).read_text())
    before.pop('current_acceptance'); after.pop('current_acceptance')
    if before != after:
        errors.append('Frozen handoff history changed during path migration')
    current = [root / p for p in ['README.md', 'AGENTS.md', 'docs/README.md',
                                   'docs/research/README.md', 'validation/README.md']]
    for directory in ['docs/guide', 'docs/development', 'docs/decisions']:
        current += sorted((root / directory).glob('*.md'))
    if sorted(p.name for p in (root / 'docs').iterdir() if p.is_file()) != ['README.md']:
        errors.append('docs root must contain only the navigation README')
    allowed_sections = {'guide', 'development', 'decisions', 'research'}
    for path in (root / 'docs').rglob('*'):
        if path.is_file():
            parts = path.relative_to(root / 'docs').parts
            if len(parts) > 1 and parts[0] not in allowed_sections:
                errors.append(f'Unexpected documentation section: {parts[0]}')
    for path in current:
        text = path.read_text()
        # Code examples may contain placeholder paths; only rendered Markdown links are navigation.
        text = re.sub(r'```.*?```', '', text, flags=re.S)
        for target in re.findall(r'\]\(([^\s)]+)\)', text):
            if re.match(r'[a-z]+://|mailto:|#', target):
                continue
            relative = unquote(target.split('#', 1)[0])
            destination = (path.parent / relative).resolve()
            if not destination.is_relative_to(root.resolve()) or not destination.exists():
                errors.append(f'Broken current link: {path.relative_to(root)} -> {target}')
    return {'scope': 'Current documentation navigation and source provenance, not language acceptance',
            'source_commit': source, 'historical_documents': len(entries),
            'current_documents_checked': len(current), 'errors': errors}


def main():
    try:
        report = check()
    except (OSError, ValueError, KeyError, StopIteration, subprocess.CalledProcessError) as error:
        print(f'Documentation check failed: {error}', file=sys.stderr)
        return 1
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return bool(report['errors'])


if __name__ == '__main__':
    sys.exit(main())

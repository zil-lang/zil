#!/usr/bin/env python3
"""Archive scoped research records, excluding executable/cache work products."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--root', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('records', nargs='+')
a = p.parse_args()
a.out.parent.mkdir(parents=True, exist_ok=True)
paths = set()
for name in a.records:
    path = a.root / name
    if not path.exists():
        raise SystemExit(f'Missing record: {path}')
    paths.update(path.rglob('*') if path.is_dir() else [path])
entries = []
with a.out.open('xb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as zipped, tarfile.open(fileobj=zipped, mode='w|') as archive:
    for path in sorted(paths):
        if not path.is_file() or path.is_symlink():
            continue
        relative = path.relative_to(a.root)
        if any(part in {'objects', 'incremental', 'cache', '__pycache__'} for part in relative.parts):
            continue
        if path.suffix in {'.o', '.so', '.rlib', '.rmeta', '.bc', '.pyc'}:
            continue
        with path.open('rb') as source:
            magic = source.read(4)
        if magic == b'\x7fELF' or magic[:2] == b'MZ':
            continue
        data = path.read_bytes()
        info = tarfile.TarInfo(str(relative))
        info.size = len(data)
        info.mode = 0o644
        archive.addfile(info, io.BytesIO(data))
        entries.append({'path': str(relative), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    data = (json.dumps({'files': entries}, indent=2) + '\n').encode()
    info = tarfile.TarInfo('FILES.json')
    info.size = len(data)
    archive.addfile(info, io.BytesIO(data))
receipt = {'archive': a.out.name, 'sha256': hashlib.sha256(a.out.read_bytes()).hexdigest(),
           'bytes': a.out.stat().st_size, 'file_count': len(entries),
           'uncompressed_bytes': sum(e['bytes'] for e in entries),
           'excluded': 'object/incremental caches, executable binaries, object/shared libraries/bitcode; source, IR, reports, native outputs, failures and timing rows retained',
           'records': a.records}
a.out.with_suffix(a.out.suffix + '.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))

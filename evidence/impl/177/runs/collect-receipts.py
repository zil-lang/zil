from pathlib import Path
import argparse
import hashlib
import json
import shutil

p = argparse.ArgumentParser()
p.add_argument('--repo', type=Path, required=True)
a = p.parse_args()
base = Path('/workspace/scratch/39be3faa4442/run-logs')
dest = a.repo / 'evidence/impl/177'
dest.mkdir(parents=True, exist_ok=False)
rows = []
skipped = {}
for name, prefix in [('177', 'runs'), ('compat177-by176', 'compatibility-review')]:
    root = base / name
    for src in sorted(root.rglob('*')):
        if not src.is_file() or src.is_symlink():
            continue
        rel = src.relative_to(root)
        reason = None
        if name == '177' and rel.parts[0].startswith('workspace') and 'cases' in rel.parts:
            reason = 'whole-workspace generated cases; exact test sources retained in Git and complete run log retained'
        elif any('phases' in part or part.endswith('-ir') for part in rel.parts[:-1]):
            reason = 'reproducible phase output'
        elif src.suffix not in {'.json', '.log', '.md', '.zil', '.rs', '.py'} and src.name not in {'stdout', 'stderr'}:
            reason = 'not a compact source or textual receipt'
        elif src.stat().st_size > 2 * 1024 * 1024:
            reason = 'over individual compact receipt limit'
        if reason:
            skipped[reason] = skipped.get(reason, 0) + 1
            continue
        raw = src.read_bytes()
        if raw.startswith((b'\x7fELF', b'MZ', b'\x00asm')) or b'\0' in raw:
            skipped['binary'] = skipped.get('binary', 0) + 1
            continue
        out = dest / prefix / rel
        if src.name in {'stdout', 'stderr'}:
            out = out.with_name(src.name + '.log')
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(raw)
        rows.append({'path': str(out.relative_to(dest)), 'original_path': str(src),
                     'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest = {'scope': '174-177 root integration and independent supplemental compatibility receipts',
            'policy': 'No native binaries, objects, LLVM outputs, build caches or generated phase dumps. Scoped source fixtures, original command records and textual stdout/stderr retained byte for byte. Whole-workspace generated cases omitted; exact test sources remain in Git and complete workspace logs are retained. Historical failed attempts remain failed.',
            'files': rows, 'skipped_by_reason': skipped,
            'selected_bytes': sum(row['bytes'] for row in rows)}
(dest / 'FILES.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'files': len(rows), 'bytes': manifest['selected_bytes'], 'skipped': skipped}))

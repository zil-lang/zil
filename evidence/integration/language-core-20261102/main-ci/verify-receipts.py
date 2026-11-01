from pathlib import Path
import gzip
import hashlib
import json
import re
import shutil
import subprocess
import sys

run = Path(sys.argv[1])
out = Path(sys.argv[2])
repo = Path('/workspace/zil')
candidate = 'e92508be996a190fa188d08319d37cc8b522bae6'
merge = 'fde8081418454acc51d9c7e937e76e063ffacd92'
receipts = run / 'selected/receipts'
meta = json.loads((run / 'run.json').read_text())
recovery = json.loads((run / 'recovery.json').read_text())
inputs = json.loads((receipts / 'source-inputs.json').read_text())
assert meta['status'] == 'completed' and meta['conclusion'] == 'success'
assert meta['headSha'] == merge == recovery['checkout'] == inputs['commit']
assert recovery['run_id'] == run.name and inputs['status'] == 'Pass'
assert inputs['checkout_before'] == inputs['checkout_after'] == 'ActualBytesTypesAndGitModesMatchHEAD'

def git(*args):
    return subprocess.check_output(['git', '-C', str(repo), *args])

tree = git('rev-parse', merge + '^{tree}').decode().strip()
assert tree == git('rev-parse', candidate + '^{tree}').decode().strip()
listing = git('ls-tree', '-r', '-l', merge)
assert listing == (receipts / 'source-tree.txt').read_bytes()
source = {}
for row in git('ls-tree', '-r', '-l', '-z', merge).split(b'\0'):
    if not row:
        continue
    info, raw_path = row.split(b'\t', 1)
    mode, kind, oid, size = info.split()
    path = raw_path.decode()
    assert kind == b'blob'
    if not path.startswith('evidence/'):
        source[path] = (mode.decode(), oid.decode(), int(size))
assert set(source) == {f['path'] for f in inputs['files']}
assert len(source) == inputs['input_files']
cat = subprocess.Popen(['git', '-C', str(repo), 'cat-file', '--batch'], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
for f in inputs['files']:
    mode, oid, size = source[f['path']]
    assert (mode, oid, size) == (f['git_mode'], f['git_oid'], f['bytes'])
    cat.stdin.write((oid + '\n').encode())
    cat.stdin.flush()
    h = cat.stdout.readline().split()
    assert h == [oid.encode(), b'blob', str(size).encode()]
    data = cat.stdout.read(size)
    assert cat.stdout.read(1) == b'\n'
    assert hashlib.sha256(data).hexdigest() == f['sha256']
cat.stdin.close()
assert cat.wait() == 0
assert hashlib.sha256((receipts / 'source-inputs.tar.gz').read_bytes()).hexdigest() == inputs['archive_sha256']

counts = {}
for name, expected in [('workspace', 3446), ('reference', 3347), ('text-reference', 64)]:
    text = (receipts / (name + '.log')).read_text()
    found = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', text)
    assert found, name
    totals = [sum(int(v[i]) for v in found) for i in range(5)]
    assert totals == [expected, 0, 0, 0, 0], (name, totals)
    counts[name] = dict(zip(['passed', 'failed', 'ignored', 'measured', 'filtered'], totals))
consumer = json.loads((receipts / 'consumer209.json').read_text())
assert consumer['status'] == 'Pass' and consumer['source_commit'] == merge
artifacts = json.loads((run / 'artifacts.json').read_text())
assert all(j['conclusion'] == 'success' for j in meta['jobs'])
assert not [(j['name'], s['name']) for j in meta['jobs'] for s in j['steps'] if s['conclusion'] in ('failure', 'cancelled')]
result = {
    'status': 'Pass', 'run_id': run.name, 'url': meta['url'], 'head': meta['headSha'],
    'actual_checkout': recovery['checkout'], 'tree': tree,
    'tree_listing_sha256': hashlib.sha256(listing).hexdigest(),
    'tree_identical_to_candidate': True, 'implementation_commit': candidate,
    'current_non_evidence_file_hashes_checked': len(source), 'mismatches': [],
    'conclusion': meta['conclusion'], 'tests': counts, 'recovery': recovery,
    'artifact_api_total_count': artifacts['total_count'],
    'raw_archive': str(run / 'evidence-recovery.tar.zst'),
    'upload_note': 'Complete log recovery verified all indexed blobs; action conclusion alone was not used as proof of successful artifact storage.',
    'consumer209_status': consumer['status'],
}
out.mkdir(parents=True, exist_ok=False)
(out / 'RESULT.json').write_text(json.dumps(result, indent=2) + '\n')
for name in ['run.json', 'recovery.json', 'artifacts.json', 'index-identity.json']:
    shutil.copyfile(run / name, out / name)
for name in ['consumer209.json', 'llvm-api-toolchain.json', 'llvm-install.json', 'toolchain.txt', 'commit.txt']:
    shutil.copyfile(receipts / name, out / name)
for name in ['workspace.log', 'reference.log', 'text-reference.log', 'clippy.log', 'fmt.log', 'migration-runner.log', 'source-inputs.json']:
    with gzip.GzipFile(filename=str(out / (name + '.gz')), mode='wb', mtime=0) as f:
        f.write((receipts / name).read_bytes())
print(json.dumps({'run': run.name, 'status': 'Pass', 'source_files': len(source), 'tests': counts}, indent=2))

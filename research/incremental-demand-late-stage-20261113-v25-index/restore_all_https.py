#!/usr/bin/env python3
"""Restore every frozen V25 regular file using fixed Git commits and fresh HTTPS."""
import argparse, hashlib, json, os, re, shutil, stat, subprocess, sys, urllib.request
from pathlib import Path, PurePosixPath

INDEX_PATH = 'research/incremental-demand-late-stage-20261113-v25-index'
CHECKPOINT = 'zil-push-main/out/evidence/full-push-checkpoint-24'
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('index_commit')
p.add_argument('--work-dir', type=Path, required=True)
p.add_argument('--restore-root', type=Path)
p.add_argument('--receipt', type=Path, required=True)
a = p.parse_args()
assert re.fullmatch('[0-9a-f]{40}', a.index_commit)

def digest(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

def safe_name(name):
    q = PurePosixPath(name)
    assert name and not q.is_absolute() and '\\' not in name
    assert all(x not in ('', '.', '..') for x in name.split('/'))
    return q

def fresh(path):
    assert not path.exists() and not path.is_symlink(), str(path)
    assert all(not parent.is_symlink() for parent in path.parents)
    path.mkdir(parents=True, mode=0o700)

def fetch(commit, relative, target, expected=None, maximum=64*1024*1024):
    assert re.fullmatch('[0-9a-f]{40}', commit)
    safe_name(relative)
    assert not target.exists()
    url = 'https://raw.githubusercontent.com/zil-lang/zil/' + commit + '/' + relative
    with urllib.request.urlopen(url, timeout=60) as r, target.open('xb') as f:
        total = 0
        while b := r.read(1024*1024):
            total += len(b)
            assert total <= maximum
            f.write(b)
    if expected:
        assert total == expected['bytes'] and digest(target) == expected['sha256']
    return target

fresh(a.work_dir)
if a.restore_root:
    fresh(a.restore_root)
    assert not a.work_dir.absolute().is_relative_to(a.restore_root.absolute())
pointer = json.loads(fetch(a.index_commit, INDEX_PATH+'/RESTORE.json', a.work_dir/'RESTORE.json').read_text())
index_file = fetch(a.index_commit, INDEX_PATH+'/INDEX.json', a.work_dir/'INDEX.json', pointer['index'])
index = json.loads(index_file.read_text())
assert index['status'] == 'all_shards_fresh_HTTPS_verified'
assert index['main_merge_or_speed_acceptance_claimed'] is False
seen = set()
actual_entries = {}
verified = []
master = structure = None
envelope = set(index['envelope_paths'])
for shard in index['shards']:
    tag = shard['tag']
    assert re.fullmatch('v25-[0-9]{3}', tag)
    root = a.work_dir/tag
    root.mkdir(mode=0o700)
    bootstrap = fetch(shard['fixed_archive_commit'], shard['archive_path']+'/restore_https.py', root/'bootstrap.py', shard['bootstrap'])
    receipt = root/'receipt.json'
    with (root/'restore.log').open('xb') as log:
        result = subprocess.run([sys.executable, str(bootstrap), shard['fixed_archive_commit'], '--download-dir', str(root/'remote'), '--restore-dir', str(root/'tree'), '--receipt', str(receipt)], stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, tag
    r = json.loads(receipt.read_text())
    assert r['status'] == 'verified' and r['verification']['files'] == shard['files']
    manifest_file = root/'remote/evidence-manifest.json'
    assert digest(manifest_file) == shard['manifest_sha256']
    manifest = json.loads(manifest_file.read_text())
    assert manifest['archive']['sha256'] == shard['archive_sha256']
    for e in manifest['inventory']['files']:
        name = e['path']; safe_name(name)
        assert name not in seen
        seen.add(name)
        actual_entries[name] = (e['bytes'], e['mode'], e['sha256'])
        f = root/'tree'/name
        st = f.lstat()
        assert stat.S_ISREG(st.st_mode) and st.st_size == e['bytes'] and stat.S_IMODE(st.st_mode) == e['mode'] and digest(f) == e['sha256']
        if name == CHECKPOINT+'/MASTER.json':
            assert digest(f) == index['master_sha256']; master = json.loads(f.read_text())
        if name == CHECKPOINT+'/STRUCTURE.json':
            assert digest(f) == index['structure_sha256']; structure = json.loads(f.read_text())
        if a.restore_root:
            target = a.restore_root/name
            target.parent.mkdir(parents=True, exist_ok=True)
            assert not target.exists() and not target.is_symlink()
            shutil.move(f, target)
    verified.append({'tag': tag, 'fixed_commit': shard['fixed_archive_commit'], 'files': shard['files'], 'receipt': str(receipt), 'receipt_sha256': digest(receipt)})
    shutil.rmtree(root/'tree'); shutil.rmtree(root/'remote')
    print(json.dumps({'verified': tag, 'files': shard['files']}), flush=True)
assert master and structure
assert len(master['files']) == index['master_files'] == master['regular_files']
assert sum(e['bytes'] for e in master['files']) == index['master_bytes']
assert seen == {e['path'] for e in master['files']} | envelope
assert len(seen) == index['union_unique_files']
for e in master['files'] + index['envelope_entries']:
    assert actual_entries[e['path']] == (e['bytes'], e['mode'], e['sha256'])
assert not structure['symlinks'] and not structure['special_files']
directory_modes = 0
if a.restore_root:
    for name, (size, mode, file_sha) in actual_entries.items():
        f = a.restore_root/name; st = f.lstat()
        assert stat.S_ISREG(st.st_mode) and st.st_size == size and stat.S_IMODE(st.st_mode) == mode and digest(f) == file_sha
    for e in structure['directories']:
        safe_name(e['path']); assert isinstance(e['mode'], int) and 0 <= e['mode'] <= 0o777
        d = a.restore_root/e['path']; d.mkdir(parents=True, exist_ok=True)
        assert d.is_dir() and not d.is_symlink()
    for e in sorted(structure['directories'], key=lambda x: -len(PurePosixPath(x['path']).parts)):
        d = a.restore_root/e['path']; d.chmod(e['mode']); assert stat.S_IMODE(d.stat().st_mode) == e['mode']; directory_modes += 1
record = {'status': 'verified', 'index_commit': a.index_commit, 'index_sha256': digest(index_file), 'master_sha256': index['master_sha256'], 'master_files': index['master_files'], 'master_bytes': index['master_bytes'], 'unique_regular_files': len(seen), 'union_exact_no_gaps_no_duplicates': True, 'directory_modes_restored': directory_modes, 'restore_root': str(a.restore_root) if a.restore_root else None, 'shards': verified, 'new_compiler_or_native_executions': 0, 'main_merge_or_speed_acceptance_claimed': False}
with a.receipt.open('x') as f:
    json.dump(record, f, indent=2); f.write('\n')
print(json.dumps(record), flush=True)

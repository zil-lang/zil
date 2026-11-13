#!/usr/bin/env python3
"""Restore every frozen V30 regular file using fixed Git commits and fresh HTTPS."""
import argparse, hashlib, json, os, re, shutil, stat, subprocess, sys, urllib.request
from pathlib import Path, PurePosixPath

def restore_reused_files(mappings, work, restore_root=None, minimum_free_bytes=None):
    """Fresh HTTPS reconstruct exact reused regular files, never execute payloads."""
    import io, tarfile, zstandard, urllib.request, hashlib, shutil, stat
    from pathlib import Path, PurePosixPath
    work = Path(work)
    assert not work.exists()
    work.mkdir(mode=0o700)
    def safe(value):
        p = PurePosixPath(value)
        assert value and not p.is_absolute() and '\\' not in value
        assert all(x not in ('', '.', '..') for x in value.split('/'))
        return p
    def digest(p):
        with p.open('rb') as f:
            return hashlib.file_digest(f, 'sha256').hexdigest()
    groups = {}
    seen = set()
    for m in mappings:
        current, stored = m['current'], m['stored']
        safe(current['path']); safe(stored['path']); safe(stored['archive_path'])
        assert current['path'] not in seen
        seen.add(current['path'])
        assert all(current[k] == stored[k] for k in ['bytes', 'mode', 'sha256'])
        key = (stored['fixed_archive_commit'], stored['archive_path'], stored['manifest_sha256'])
        groups.setdefault(key, {}).setdefault(stored['path'], []).append(current)
    records, verified_entries = [], {}
    min_free = shutil.disk_usage(work).free
    for number, ((commit, archive_path, manifest_sha), wanted) in enumerate(groups.items()):
        assert len(commit) == 40 and all(c in '0123456789abcdef' for c in commit)
        base = 'https://raw.githubusercontent.com/zil-lang/zil/' + commit + '/' + archive_path + '/'
        with urllib.request.urlopen(base+'evidence-manifest.json', timeout=60) as r:
            body = r.read(64*1024*1024+1)
        assert len(body) <= 64*1024*1024 and hashlib.sha256(body).hexdigest() == manifest_sha
        import json
        manifest = json.loads(body)
        entries = {e['path']: e for e in manifest['inventory']['files']}
        for path, currents in wanted.items():
            e = entries[path]
            assert all(all(c[k] == e[k] for k in ['bytes', 'mode', 'sha256']) for c in currents)
        parts = manifest['archive']['parts']
        class Parts(io.RawIOBase):
            def __init__(self):
                self.i=0; self.r=None; self.h=None; self.n=0; self.total=0; self.all=hashlib.sha256(); self.checked=[]
            def readable(self): return True
            def read(self, size=-1):
                size = 1024*1024 if size < 0 else size
                if size == 0: return b''
                while True:
                    if self.r is None:
                        if self.i >= len(parts): return b''
                        safe(parts[self.i]['name'])
                        self.r=urllib.request.urlopen(base+parts[self.i]['name'],timeout=60)
                        self.h=hashlib.sha256(); self.n=0
                    b=self.r.read(size)
                    if b:
                        self.h.update(b); self.all.update(b); self.n+=len(b); self.total+=len(b)
                        return b
                    self.r.close(); self.r=None
                    part=parts[self.i]
                    assert self.n==part['bytes'] and self.h.hexdigest()==part['sha256']
                    self.checked.append(part); self.i+=1
            def readinto(self, b):
                q=self.read(len(b)); b[:len(q)]=q
                return len(q)
        reader=Parts(); done=set(); group=work/('group-'+str(number)); group.mkdir(mode=0o700)
        with zstandard.ZstdDecompressor().stream_reader(reader) as decompressed:
            with tarfile.open(fileobj=decompressed, mode='r|') as tf:
                for member in tf:
                    old_path=member.name.removeprefix('payload/')
                    if old_path not in wanted: continue
                    assert old_path not in done
                    done.add(old_path); e=entries[old_path]
                    assert member.isreg() and member.size==e['bytes'] and member.mode==e['mode']
                    temp=group/'fresh-source'; assert not temp.exists()
                    h=hashlib.sha256(); count=0
                    with tf.extractfile(member) as source, temp.open('xb') as out:
                        while b:=source.read(1024*1024):
                            out.write(b); h.update(b); count+=len(b)
                    temp.chmod(e['mode'])
                    assert count==e['bytes'] and h.hexdigest()==e['sha256']
                    for current in wanted[old_path]:
                        root=Path(restore_root) if restore_root else group/'logical-tree'
                        target=root/current['path']; target.parent.mkdir(parents=True, exist_ok=True)
                        assert not target.exists() and not target.is_symlink()
                        shutil.copyfile(temp,target); target.chmod(current['mode'])
                        s=target.lstat()
                        assert stat.S_ISREG(s.st_mode) and s.st_size==current['bytes'] and stat.S_IMODE(s.st_mode)==current['mode'] and digest(target)==current['sha256']
                        verified_entries[current['path']] = (current['bytes'],current['mode'],current['sha256'])
                        min_free=min(min_free,shutil.disk_usage(work).free)
                        if minimum_free_bytes is not None: assert min_free > minimum_free_bytes
                        if restore_root is None: target.unlink()
                    temp.unlink()
            while decompressed.read(1024*1024): pass
        assert done==set(wanted) and reader.i==len(parts)
        assert reader.total==manifest['archive']['bytes'] and reader.all.hexdigest()==manifest['archive']['sha256']
        records.append({'fixed_commit':commit,'archive_path':archive_path,'manifest_sha256':manifest_sha,'archive_sha256':reader.all.hexdigest(),'all_parts_verified':True,'stored_selected_files':len(done),'logical_files_restored':sum(len(v) for v in wanted.values())})
        shutil.rmtree(group)
    assert set(verified_entries)==seen
    return {'status':'verified','logical_files':len(seen),'logical_bytes':sum(e[0] for e in verified_entries.values()),'entries':verified_entries,'groups':records,'min_observed_free_bytes':min_free,'all_logical_alias_bytes_size_mode_fresh_materialized_and_verified':True}

INDEX_PATH = 'research/incremental-demand-full-chain-20261113-v30-index'
CHECKPOINT = 'zil-push-main/out/evidence/full-push-checkpoint-30'
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
    assert re.fullmatch('v30-[0-9]{3}', tag)
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
reused = restore_reused_files(index.get('reused_regular_files', []), a.work_dir/'reused-fixed-files', a.restore_root)
for name, values in reused['entries'].items():
    assert name not in seen
    seen.add(name); actual_entries[name] = tuple(values)
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
record = {'status': 'verified', 'index_commit': a.index_commit, 'index_sha256': digest(index_file), 'master_sha256': index['master_sha256'], 'master_files': index['master_files'], 'master_bytes': index['master_bytes'], 'unique_regular_files': len(seen), 'union_exact_no_gaps_no_duplicates': True, 'directory_modes_restored': directory_modes, 'restore_root': str(a.restore_root) if a.restore_root else None, 'shards': verified, 'reused_regular_files': reused['logical_files'], 'reused_logical_bytes': reused['logical_bytes'], 'reused_fresh_groups': reused['groups'], 'new_compiler_or_native_executions': 0, 'main_merge_or_speed_acceptance_claimed': False}
with a.receipt.open('x') as f:
    json.dump(record, f, indent=2); f.write('\n')
print(json.dumps(record), flush=True)

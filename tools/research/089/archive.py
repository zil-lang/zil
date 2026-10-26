#!/usr/bin/env python3
"""Portable exact raw archive, with a per-file digest and execution-mode census."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda:stream.read(1024*1024),b''): h.update(block)
    return h.hexdigest()
def archive(root,output):
    root=root.resolve(); output=output.absolute()
    assert root.is_dir() and not output.exists() and not output.is_relative_to(root)
    files=[]
    for path in sorted(root.rglob('*')):
        if path.is_symlink(): raise ValueError(f'symlink in raw census: {path}')
        if path.is_file():
            files.append(dict(path='raw/'+str(path.relative_to(root)),size=path.stat().st_size,sha256=digest(path),mode=path.stat().st_mode & 0o777))
    manifest=dict(schema_version=1,scope='All files under the explicitly selected 089 raw root; not every earlier project run',files=files)
    data=(json.dumps(manifest,ensure_ascii=False,sort_keys=True,indent=2)+'\n').encode()
    output.parent.mkdir(parents=True,exist_ok=True)
    with output.open('xb') as raw, gzip.GzipFile(fileobj=raw,mode='wb',mtime=0,compresslevel=6) as zipped, tarfile.open(fileobj=zipped,mode='w') as tar:
        info=tarfile.TarInfo('raw/archive-manifest.json'); info.size=len(data); info.mode=0o644; tar.addfile(info,io.BytesIO(data))
        for row in files:
            path=root/row['path'][4:]
            info=tarfile.TarInfo(row['path']); info.size=row['size']; info.mode=row['mode']
            with path.open('rb') as stream: tar.addfile(info,stream)
    return verify(output)
def verify(path):
    with tarfile.open(path,'r:gz') as tar:
        members=tar.getmembers(); names=[m.name for m in members]
        assert len(names)==len(set(names))
        assert all(m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts for m in members)
        manifest=json.load(tar.extractfile('raw/archive-manifest.json'))
        expected={row['path']:row for row in manifest['files']}
        assert set(names)==set(expected)|{'raw/archive-manifest.json'}
        for member in members:
            if member.name=='raw/archive-manifest.json': continue
            row=expected[member.name]; h=hashlib.sha256(); size=0
            with tar.extractfile(member) as stream:
                for block in iter(lambda:stream.read(1024*1024),b''): h.update(block); size+=len(block)
            assert size==row['size']==member.size and h.hexdigest()==row['sha256'] and member.mode==row['mode'],member.name
    return dict(status='Pass',archive=str(path),sha256=digest(path),bytes=path.stat().st_size,members=len(members),payload_files=len(expected),uncompressed_bytes=sum(r['size'] for r in expected.values()))
if __name__=='__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('command',choices=['create','verify']); parser.add_argument('--root',type=Path); parser.add_argument('--archive',type=Path,required=True); args=parser.parse_args()
    print(json.dumps(archive(args.root,args.archive) if args.command=='create' else verify(args.archive),ensure_ascii=False,indent=2))

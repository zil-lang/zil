#!/usr/bin/env python3
"""Verify all archive blobs, then restore complete consumer256 originals and receipts.

Original JSON bytes and logical paths are never rewritten. No compiler/native
execution. A restoration Pass is not a source/CI/consumer acceptance verdict.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tarfile

from audit_run import git, need, pin_transport, safe_relative, sha, write_json


def selected(path):
    return path.startswith('receipts/') or (
        path.startswith('native-evidence/') and
        any(part.startswith('consumer256-') for part in PurePosixPath(path).parts))


def restore(args, result, out):
    archive=args.archive.resolve()
    before=archive.stat()
    expected=git(args.repo,'rev-parse',args.expected_head).decode().strip()
    producer,path=pin_transport(args.repo,expected,out)
    # Reuse the audited bounded archive parser, including every unique blob.
    index=producer.verify(archive)
    need(index['producer_sha256']==sha(path),'Pinned producer does not match archive')
    write_json(out/'INDEX.json',index)
    chosen=[entry for entry in index['files'] if selected(entry['path'])]
    need(any(e['path'].startswith('receipts/') for e in chosen),'No receipts in archive')
    need(any('consumer256-' in e['path'] for e in chosen),'No consumer256 originals in archive')
    logical=sum(e['bytes'] for e in chosen)
    need(logical<=args.max_selected_gib*1024**3,'Selected restoration exceeds explicit disk budget')
    need(shutil.disk_usage(out).free>=logical+512*1024**2,'Insufficient free space for selected originals')
    for e in chosen:
        safe_relative(e['path'])
        need(type(e['mode']) is int and 0<=e['mode']<=0o777,'Invalid selected ordinary file mode')
    roots=out/'restored';roots.mkdir()
    by_hash={}
    for e in chosen:by_hash.setdefault(e['sha256'],[]).append(e)
    found=set()
    proc=subprocess.Popen(['zstd','-q','-d','--memory=64MB','-c',str(archive)],stdout=subprocess.PIPE)
    try:
        with tarfile.open(fileobj=proc.stdout,mode='r|',ignore_zeros=True) as tf:
            for member in tf:
                if not member.name.startswith('objects/'):continue
                digest=member.name[8:]
                if digest not in by_hash:continue
                need(digest not in found,'Duplicate selected object')
                entries=by_hash[digest]
                need(member.isfile() and member.size==entries[0]['bytes'],'Selected object type/size mismatch')
                target=roots/entries[0]['path'];target.parent.mkdir(parents=True,exist_ok=True)
                with tf.extractfile(member) as source,target.open('xb') as dest:
                    shutil.copyfileobj(source,dest,1024*1024)
                need(sha(target)==digest,'Selected object SHA mismatch')
                os.chmod(target,entries[0]['mode'])
                for e in entries[1:]:
                    other=roots/e['path'];other.parent.mkdir(parents=True,exist_ok=True)
                    with target.open('rb') as source,other.open('xb') as dest:
                        shutil.copyfileobj(source,dest,1024*1024)
                    os.chmod(other,e['mode'])
                found.add(digest)
        while proc.stdout.read(1024*1024):pass
        need(proc.wait()==0 and found==set(by_hash),'Selected extraction incomplete')
    finally:
        if proc.poll() is None:proc.kill()
        proc.stdout.close();proc.wait()
    for e in chosen:
        p=roots/e['path'];s=p.stat()
        need(s.st_size==e['bytes'] and stat.S_IMODE(s.st_mode)==e['mode'] and sha(p)==e['sha256'],
             'Restored file mode/size/SHA mismatch: '+e['path'])
    after=archive.stat()
    need((before.st_size,before.st_mtime_ns)==(after.st_size,after.st_mtime_ns),'Archive changed during restoration')
    write_json(out/'SELECTED.json',{'files':chosen,'selection':'receipts/* plus any consumer256-* component under native-evidence/'})
    result.update(status='Pass',expected_head=expected,pinned_producer_sha256=sha(path),
                  archive=str(archive),archive_sha256=sha(archive),archive_bytes=after.st_size,
                  full_index_files=len(index['files']),all_content_addressed_blobs='Pass',
                  selected_files=len(chosen),selected_logical_bytes=logical,unique_selected_objects=len(found),
                  actual_restored_modes_sizes_sha256='Pass',index_run_id=index.get('run_id'),
                  index_checkout=index.get('checkout'),index_missing_roots=index.get('missing_roots'),
                  restored_root=str(roots),full_index=str(out/'INDEX.json'),selection_index=str(out/'SELECTED.json'),
                  original_json_rewritten=False,
                  scope='Selected file restoration only; source execution identity and grouped consumer acceptance are separate collector/audit checks.')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive',type=Path,required=True)
    p.add_argument('--repo',type=Path,required=True)
    p.add_argument('--expected-head',required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--max-selected-gib',type=int,default=2)
    args=p.parse_args()
    out=args.output.resolve()
    need(out.is_relative_to(Path('/workspace/.zil-env/forward')),'Output must stay in the forward evidence workspace')
    need(args.max_selected_gib>0,'Invalid selected-space budget')
    out.mkdir(parents=True,exist_ok=False)
    result={'status':'Running','script_sha256':sha(Path(__file__)),'auditor':'/root/assess_delivery_process'}
    try:restore(args,result,out)
    except Exception as exc:result.update(status='Fail',error=str(exc),error_type=type(exc).__name__)
    write_json(out/'RESTORE.json',result)
    print(json.dumps({k:result.get(k) for k in ('status','restored_root','full_index','selected_files','selected_logical_bytes','error')}))
    return 0 if result['status']=='Pass' else 1


if __name__=='__main__':sys.exit(main())

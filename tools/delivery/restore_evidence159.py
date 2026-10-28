#!/usr/bin/env python3
"""Restore content-addressed evidence, or stream-verify without expanding it."""
import argparse,pathlib,json,hashlib,zipfile,stat,shutil
p=argparse.ArgumentParser();p.add_argument('--archive',required=True);p.add_argument('--destination');p.add_argument('--verify-only',action='store_true');a=p.parse_args();archive=pathlib.Path(a.archive)
if not a.verify_only:assert a.destination,'destination required';dst=pathlib.Path(a.destination).resolve();assert not dst.exists(),'refusing overwrite';dst.mkdir(parents=True)
with zipfile.ZipFile(archive) as z:
 m=json.loads(z.read('MANIFEST.json'));count=0;total=0
 for sha,meta in m['blobs'].items():
  h=hashlib.sha256();n=0
  with z.open('blobs/'+sha) as f:
   for b in iter(lambda:f.read(1024*1024),b''):h.update(b);n+=len(b)
  assert n==meta['bytes'] and h.hexdigest()==sha,sha
 for name,meta in m['files'].items():
  path=pathlib.PurePosixPath(name);assert not path.is_absolute() and '..' not in path.parts,name
  if meta['kind']!='file':continue # symlink targets remain auditable metadata, never followed/created
  count+=1;total+=meta['bytes']
  if not a.verify_only:
   out=dst.joinpath(*path.parts);out.parent.mkdir(parents=True,exist_ok=True)
   with z.open('blobs/'+meta['sha256']) as f,out.open('wb') as g:shutil.copyfileobj(f,g,1024*1024)
   out.chmod(meta['mode'])
 print(json.dumps({'status':'Pass','unique_blobs':len(m['blobs']),'logical_files':count,'logical_bytes':total,'mode':'stream-verify'if a.verify_only else'restore','nonregular':'metadata only, not materialized'},indent=2))

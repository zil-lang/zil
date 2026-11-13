"""Exact-byte V32 CAS HTTPS verifier; payloads are never executed."""
from pathlib import Path,PurePosixPath
import io,json,hashlib,urllib.request,tarfile,stat,shutil,os
import zstandard
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def safe(s):
 assert s and not s.startswith('/') and '\\' not in s and all(x not in ('','.','..') for x in s.split('/'));return s
def materialize(temp,o,entries,work,restore_root=None,reserve=500*1024*1024):
 canonical={};minimum=shutil.disk_usage(work).free
 for name in o['logical_paths']:
  safe(name);e=entries[name];target=(Path(restore_root) if restore_root else work/'logical-tree')/name;target.parent.mkdir(parents=True,exist_ok=True);assert not target.exists() and not target.is_symlink()
  if restore_root:
   if e['mode'] in canonical:os.link(canonical[e['mode']],target)
   else:
    assert shutil.disk_usage(target.parent).free>e['bytes']+reserve;shutil.copyfile(temp,target);target.chmod(e['mode']);canonical[e['mode']]=target
  else:temp.chmod(e['mode']);os.link(temp,target)
  st=target.lstat();assert stat.S_ISREG(st.st_mode) and st.st_size==e['bytes'] and stat.S_IMODE(st.st_mode)==e['mode'] and sha(target)==e['sha256']
  minimum=min(minimum,shutil.disk_usage(work).free);assert minimum>reserve
  if not restore_root:target.unlink()
 return minimum
def verify_legacy(objects,entries,work,callback=None,reserve=500*1024*1024,restore_root=None):
 work=Path(work);work.mkdir(exist_ok=False);groups={};records=[];minimum=shutil.disk_usage(work).free
 for o in objects:
  if o['storage']!='legacy-fixed-member':continue
  s=o['stored'];k=(s['fixed_archive_commit'],s['archive_path'],s['manifest_sha256']);groups.setdefault(k,[]).append(o)
 # Smallest groups first permit bounded storage proof before the large runtime DSOs.
 for number,(key,obs) in enumerate(sorted(groups.items(),key=lambda kv:max(x['bytes'] for x in kv[1]))):
  commit,rel,mh=key;assert len(commit)==40 and all(c in '0123456789abcdef' for c in commit);safe(rel)
  base=f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{rel}/';body=urllib.request.urlopen(base+'evidence-manifest.json',timeout=60).read(64*1024*1024+1);assert hashlib.sha256(body).hexdigest()==mh
  manifest=json.loads(body);inventory={e['path']:e for e in manifest['inventory']['files']};wanted={o['stored']['path']:o for o in obs};assert len(wanted)==len(obs)
  for p,o in wanted.items():
   e=inventory[p];assert (e['bytes'],e['sha256'],e['mode'])==(o['stored']['bytes'],o['stored']['sha256'],o['stored']['mode'])
  parts=manifest['archive']['parts']
  class Parts(io.RawIOBase):
   def __init__(self):self.i=0;self.r=None;self.h=None;self.n=0;self.total=0;self.all=hashlib.sha256()
   def readable(self):return True
   def read(self,size=-1):
    size=1024*1024 if size<0 else size
    if not size:return b''
    while True:
     if self.r is None:
      if self.i==len(parts):return b''
      safe(parts[self.i]['name']);self.r=urllib.request.urlopen(base+parts[self.i]['name'],timeout=60);self.h=hashlib.sha256();self.n=0
     b=self.r.read(size)
     if b:self.h.update(b);self.all.update(b);self.n+=len(b);self.total+=len(b);return b
     self.r.close();self.r=None;assert self.n==parts[self.i]['bytes'] and self.h.hexdigest()==parts[self.i]['sha256'];self.i+=1
   def readinto(self,b):q=self.read(len(b));b[:len(q)]=q;return len(q)
  reader=Parts();done=set();verified=[]
  with zstandard.ZstdDecompressor().stream_reader(reader) as decompressed:
   with tarfile.open(fileobj=decompressed,mode='r|') as tf:
    for member in tf:
     p=member.name.removeprefix('payload/')
     if p not in wanted:continue
     assert p not in done;done.add(p);o=wanted[p];assert member.isreg() and member.size==o['bytes'] and member.mode==o['stored']['mode']
     temp=work/'fresh-member';assert not temp.exists();assert shutil.disk_usage(work).free>o['bytes']+reserve
     with tf.extractfile(member) as src,temp.open('xb') as out:shutil.copyfileobj(src,out,1024*1024)
     assert temp.stat().st_size==o['bytes'] and sha(temp)==o['sha256']
     minimum=min(minimum,materialize(temp,o,entries,work,restore_root,reserve))
     temp.unlink();verified.append(o['sha256'])
   while decompressed.read(1024*1024):pass
  assert done==set(wanted) and reader.i==len(parts) and reader.total==manifest['archive']['bytes'] and reader.all.hexdigest()==manifest['archive']['sha256']
  record={'fixed_commit':commit,'archive_path':rel,'manifest_sha256':mh,'archive_sha256':reader.all.hexdigest(),'all_compressed_parts_and_join_verified':True,'objects':verified,'logical_files_fresh_materialized':sum(len(o['logical_paths']) for o in obs),'min_observed_free_bytes':minimum}
  if callback:callback(obs,record)
  records.append(record);print(json.dumps({'stage':'legacy-group-fresh','group':number+1,'groups':len(groups),'objects':len(obs),'paths':record['logical_files_fresh_materialized'],'free':shutil.disk_usage(work).free}),flush=True)
 shutil.rmtree(work);return {'status':'verified','groups':records,'objects':sum(len(x['objects']) for x in records),'logical_paths':sum(x['logical_files_fresh_materialized'] for x in records),'min_observed_free_bytes':minimum,'physical_temp_mode_changes_only':True,'every_logical_path_full_fresh_bytes_size_mode_verified':True}

def verify_new_chunk(commit,rel,manifest_sha,objects,entries,work,reserve=500*1024*1024,restore_root=None):
 work=Path(work);assert not work.exists();work.mkdir();safe(rel);base=f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{rel}/'
 body=urllib.request.urlopen(base+'CHUNK.json',timeout=60).read();assert hashlib.sha256(body).hexdigest()==manifest_sha;manifest=json.loads(body)
 expected={o['sha256']:o for o in objects};assert set(expected)=={e['sha256'] for e in manifest['objects']}
 for e in manifest['objects']:assert (e['bytes'],e['member'])==(expected[e['sha256']]['bytes'],'objects/'+e['sha256'])
 class Reader(io.RawIOBase):
  def __init__(self):self.r=urllib.request.urlopen(base+safe(manifest['archive']['name']),timeout=60);self.h=hashlib.sha256();self.n=0
  def readable(self):return True
  def read(self,n=-1):
   b=self.r.read(1024*1024 if n<0 else n);self.h.update(b);self.n+=len(b);return b
  def readinto(self,b):q=self.read(len(b));b[:len(q)]=q;return len(q)
 reader=Reader();done=set();minimum=shutil.disk_usage(work).free;paths=0
 with zstandard.ZstdDecompressor().stream_reader(reader) as decoded:
  with tarfile.open(fileobj=decoded,mode='r|') as tf:
   for member in tf:
    assert member.isreg() and member.name.startswith('objects/');h=member.name[len('objects/'):];assert h in expected and h not in done;done.add(h);o=expected[h];assert member.size==o['bytes'];temp=work/'fresh-member';assert not temp.exists() and shutil.disk_usage(work).free>o['bytes']+reserve
    with tf.extractfile(member) as source,temp.open('xb') as out:shutil.copyfileobj(source,out,1024*1024)
    assert temp.stat().st_size==o['bytes'] and sha(temp)==h
    minimum=min(minimum,materialize(temp,o,entries,work,restore_root,reserve));paths+=len(o['logical_paths'])
    temp.unlink()
  while decoded.read(1024*1024):pass
 while reader.read(1024*1024):pass
 reader.r.close();assert done==set(expected) and reader.n==manifest['archive']['bytes'] and reader.h.hexdigest()==manifest['archive']['sha256'];shutil.rmtree(work)
 return {'status':'verified','fixed_commit':commit,'archive_path':rel,'manifest_sha256':manifest_sha,'archive_sha256':reader.h.hexdigest(),'archive_bytes':reader.n,'objects':len(done),'logical_paths':paths,'every_logical_path_full_fresh_bytes_size_mode_verified':True,'min_observed_free_bytes':minimum}


def main():
 import argparse
 ap=argparse.ArgumentParser(description='Fresh HTTPS exact V32 CAS recovery. No payload execution. Python zstandard required.')
 ap.add_argument('fixed_index_commit');ap.add_argument('--work-dir',required=True);ap.add_argument('--restore-root');ap.add_argument('--receipt',required=True);ap.add_argument('--reserve-bytes',type=int,default=500*1024*1024)
 args=ap.parse_args();commit=args.fixed_index_commit;assert len(commit)==40 and all(c in '0123456789abcdef' for c in commit);work=Path(args.work_dir);assert not work.exists();work.mkdir();restore=Path(args.restore_root) if args.restore_root else None
 if restore:assert not restore.exists();restore.mkdir()
 rel='research/incremental-demand-full-chain-20261113-v32-index';base=f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{rel}/'
 def fetch(name):return urllib.request.urlopen(base+safe(name),timeout=60).read(64*1024*1024+1)
 pointer=json.loads(fetch('RESTORE.json'));body=fetch(pointer['metadata_manifest']['path']);assert hashlib.sha256(body).hexdigest()==pointer['metadata_manifest']['sha256'];manifest=json.loads(body);metadata=work/'metadata';metadata.mkdir();checked=[]
 for e in manifest['files']:
  data=fetch(e['path']);assert len(data)==e['bytes'] and hashlib.sha256(data).hexdigest()==e['sha256'];p=metadata/safe(e['path']);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data);p.chmod(e['mode']);assert sha(p)==e['sha256'] and stat.S_IMODE(p.stat().st_mode)==e['mode'];assert shutil.disk_usage(work).free>args.reserve_bytes;checked.append(e['path'])
 index=json.loads((metadata/'INDEX.json').read_text());master=json.loads((metadata/index['master']['path']).read_text());assert sha(metadata/index['master']['path'])==index['master']['sha256'];entries={e['path']:e for e in master['files']};catalog=json.loads((metadata/'CAS.json').read_text());objects=catalog['objects'];assert {p for o in objects for p in o['logical_paths']}==set(entries) and sum(len(o['logical_paths']) for o in objects)==len(entries)
 old=verify_legacy(objects,entries,work/'legacy-fresh',reserve=args.reserve_bytes,restore_root=restore);new=[]
 for chunk in catalog['new_chunks']:
  selected=[o for o in objects if o['sha256'] in set(chunk['objects'])];assert len(selected)==len(chunk['objects']);new.append(verify_new_chunk(chunk['fixed_commit'],chunk['archive_path'],chunk['manifest_sha256'],selected,entries,work/(chunk['tag']+'-fresh'),reserve=args.reserve_bytes,restore_root=restore))
 restored_metadata=[]
 if restore:
  for e in manifest['files']:
   if not e.get('restore_path'):continue
   target=restore/safe(e['restore_path']);target.parent.mkdir(parents=True,exist_ok=True);origin=metadata/e['path']
   if target.exists():assert target.is_file() and sha(target)==e['sha256'] and target.stat().st_size==e['bytes'] and stat.S_IMODE(target.stat().st_mode)==e['mode']
   else:
    assert shutil.disk_usage(restore).free>e['bytes']+args.reserve_bytes;shutil.copyfile(origin,target);target.chmod(e['mode']);assert sha(target)==e['sha256']
   restored_metadata.append(e['restore_path'])
  structure=json.loads((metadata/index['structure']['path']).read_text())
  for d in sorted(structure['directories'],key=lambda x:len(x['path'].split('/')),reverse=True):
   target=restore/safe(d['path']);target.mkdir(parents=True,exist_ok=True);target.chmod(d['mode']);assert stat.S_IMODE(target.stat().st_mode)==d['mode']
  wanted=set(entries)|set(restored_metadata);actual={str(p.relative_to(restore)) for p in restore.rglob('*') if p.is_file()};assert actual==wanted
 result={'status':'verified','fixed_index_commit':commit,'metadata_files_fresh_verified':len(checked),'original_logical_files':len(entries),'original_logical_bytes':master['logical_bytes'],'legacy_actual_fresh':old,'new_actual_fresh':new,'restore_root':str(restore) if restore else None,'metadata_paths_restored':len(set(restored_metadata)),'every_original_path_fresh_bytes_size_mode_verified':True,'no_payloads_executed':True,'dependencies':index['dependencies'],'boundary':master['boundary']}
 Path(args.receipt).write_text(json.dumps(result,sort_keys=True,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['legacy_actual_fresh','new_actual_fresh','dependencies','boundary']}))
if __name__=='__main__':main()

"""Exact-byte V32 CAS HTTPS verifier; payloads are never executed."""
from pathlib import Path,PurePosixPath
import io,json,hashlib,urllib.request,tarfile,stat,shutil,os
import zstandard
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def safe(s):
 assert s and not s.startswith('/') and '\\' not in s and all(x not in ('','.','..') for x in s.split('/'));return s
def verify_legacy(objects,entries,work,callback=None,reserve=500*1024*1024):
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
     for name in o['logical_paths']:
      safe(name);e=entries[name];target=work/'logical-tree'/name;target.parent.mkdir(parents=True,exist_ok=True);assert not target.exists();temp.chmod(e['mode']);os.link(temp,target)
      st=target.lstat();assert stat.S_ISREG(st.st_mode) and st.st_size==e['bytes'] and stat.S_IMODE(st.st_mode)==e['mode'] and sha(target)==e['sha256'];target.unlink()
      minimum=min(minimum,shutil.disk_usage(work).free);assert minimum>reserve
     temp.unlink();verified.append(o['sha256'])
   while decompressed.read(1024*1024):pass
  assert done==set(wanted) and reader.i==len(parts) and reader.total==manifest['archive']['bytes'] and reader.all.hexdigest()==manifest['archive']['sha256']
  record={'fixed_commit':commit,'archive_path':rel,'manifest_sha256':mh,'archive_sha256':reader.all.hexdigest(),'all_compressed_parts_and_join_verified':True,'objects':verified,'logical_files_fresh_materialized':sum(len(o['logical_paths']) for o in obs),'min_observed_free_bytes':minimum}
  if callback:callback(obs,record)
  records.append(record);print(json.dumps({'stage':'legacy-group-fresh','group':number+1,'groups':len(groups),'objects':len(obs),'paths':record['logical_files_fresh_materialized'],'free':shutil.disk_usage(work).free}),flush=True)
 shutil.rmtree(work);return {'status':'verified','groups':records,'objects':sum(len(x['objects']) for x in records),'logical_paths':sum(x['logical_files_fresh_materialized'] for x in records),'min_observed_free_bytes':minimum,'physical_temp_mode_changes_only':True,'every_logical_path_full_fresh_bytes_size_mode_verified':True}

def verify_new_chunk(commit,rel,manifest_sha,objects,entries,work,reserve=500*1024*1024):
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
    for name in o['logical_paths']:
     e=entries[name];target=work/'logical-tree'/safe(name);target.parent.mkdir(parents=True,exist_ok=True);assert not target.exists();temp.chmod(e['mode']);os.link(temp,target);st=target.lstat();assert stat.S_ISREG(st.st_mode) and st.st_size==e['bytes'] and stat.S_IMODE(st.st_mode)==e['mode'] and sha(target)==h;target.unlink();paths+=1;minimum=min(minimum,shutil.disk_usage(work).free);assert minimum>reserve
    temp.unlink()
  while decoded.read(1024*1024):pass
 while reader.read(1024*1024):pass
 reader.r.close();assert done==set(expected) and reader.n==manifest['archive']['bytes'] and reader.h.hexdigest()==manifest['archive']['sha256'];shutil.rmtree(work)
 return {'status':'verified','fixed_commit':commit,'archive_path':rel,'manifest_sha256':manifest_sha,'archive_sha256':reader.h.hexdigest(),'archive_bytes':reader.n,'objects':len(done),'logical_paths':paths,'every_logical_path_full_fresh_bytes_size_mode_verified':True,'min_observed_free_bytes':minimum}

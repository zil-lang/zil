#!/usr/bin/env python3
"""Fresh HTTPS-only restore of v19-01 at one explicit complete archive Git commit."""
import argparse,hashlib,json,os,re,subprocess,sys,time,urllib.request
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('commit');p.add_argument('--download-dir',type=Path,default=Path('/tmp/zil-full-push-v19-01-remote'));p.add_argument('--restore-dir',type=Path,default=Path('/tmp/zil-full-push-v19-01-restored'));p.add_argument('--receipt',type=Path,required=True)
a=p.parse_args()
if not re.fullmatch('[0-9a-f]{40}',a.commit):p.error('use the complete immutable Git commit')
for path in [a.download_dir,a.restore_dir]:
 if path.exists() or path.is_symlink():p.error('fresh destinations must not exist: '+str(path))
 for parent in path.parents:
  if parent.is_symlink():p.error('symlink parent rejected')
a.download_dir.mkdir(mode=0o700)
base='https://raw.githubusercontent.com/zil-lang/zil/'+a.commit+'/research/incremental-demand-full-push-20261112-v19-01/'
transfers=[]
def fetch(name, expected=None, maximum=64*1024*1024):
 if Path(name).name!=name or not re.fullmatch('[A-Za-z0-9_.-]+',name):raise ValueError('unsafe remote member')
 target=a.download_dir/name
 if target.exists():raise ValueError('refuse existing download')
 url=base+name
 for attempt in range(5):
  temporary=a.download_dir/('.download-'+name)
  try:
   h=hashlib.sha256();size=0
   with urllib.request.urlopen(url,timeout=60) as response,temporary.open('wb') as f:
    while block:=response.read(1024*1024):
     size+=len(block)
     if size>maximum:raise ValueError('remote member exceeds bound')
     h.update(block);f.write(block)
   digest=h.hexdigest()
   if expected and (digest!=expected['sha256'] or size!=expected['bytes']):raise ValueError('remote hash/size mismatch: '+name)
   temporary.replace(target);transfers.append({'url':url,'bytes':size,'sha256':digest})
   print('HTTPS',name,size,digest,flush=True);return target
  except Exception:
   temporary.unlink(missing_ok=True)
   if attempt==4:raise
   time.sleep(attempt+1)
pointer=json.loads(fetch('RESTORE.json',maximum=1024*1024).read_text())
manifest_path=fetch('evidence-manifest.json',pointer['manifest'])
manifest=json.loads(manifest_path.read_text())
assert manifest['archive']['sha256']==pointer['archive_sha256']
tool=fetch('archive_evidence.py',pointer['verifier'])
for part in manifest['archive']['parts']:fetch(part['name'],part,maximum=24*1024*1024)
# Freshly downloaded verifier: no local repository, alternates, or original raw.
result=subprocess.run([sys.executable,str(tool),'verify','--archive-dir',str(a.download_dir),'--restore-dir',str(a.restore_dir)],capture_output=True,text=True)
record={'status':'failed' if result.returncode else 'verified','fixed_archive_commit':a.commit,'base_url':base,'transfers':transfers,'verifier_stdout':result.stdout,'verifier_stderr':result.stderr,'returncode':result.returncode}
if result.returncode==0:
 record['verification']=json.loads(result.stdout)
 for entry in manifest['inventory']['files']:
  path=a.restore_dir/entry['path']
  mode=path.lstat().st_mode
  if path.is_symlink() or mode & 0o777 != entry['mode']:
   raise ValueError('post-restore mode/type mismatch: '+entry['path'])
 record['post_restore_modes_verified']=len(manifest['inventory']['files'])
a.receipt.parent.mkdir(parents=True,exist_ok=True)
with a.receipt.open('x') as f:json.dump(record,f,indent=2);f.write('\n')
if result.returncode:sys.exit(result.returncode)
print(json.dumps(record['verification']),flush=True)

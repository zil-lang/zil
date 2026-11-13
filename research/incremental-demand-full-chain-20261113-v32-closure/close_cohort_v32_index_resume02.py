from pathlib import Path
import os,stat,json,hashlib,shutil,subprocess,urllib.request,time
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-32';A=W/'zil-demand-archive';REF='refs/heads/archive/source-history';REL='research/incremental-demand-full-chain-20261113-v32-index';DOC='59fd0df5cf98cbb0dd57cbbdfaa3a8fe8160d628';GOAL='8821e9c9d966649f67650afc741aca56b4c19b33'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def action(args):
 p=subprocess.run(args,capture_output=True,text=True)
 with (S/'FINAL-OPERATIONS.jsonl').open('a') as f:f.write(json.dumps({'argv':args,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,p.stderr;return p.stdout.strip()
def git(*args):return action(['git','-C',str(A),*args])
def fetch(commit,rel,name,expected):
 body=urllib.request.urlopen(f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{rel}/{name}',timeout=60).read(64*1024*1024+1);assert len(body)==expected['bytes'] and hashlib.sha256(body).hexdigest()==expected['sha256'];return body
assert not git('status','--porcelain')
dest=A/REL;manifest=json.loads((dest/'METADATA-MANIFEST.json').read_text())['files']
git('merge','-s','ours','--no-ff','--allow-unrelated-histories',DOC,GOAL,'-m','Preserve V32 exact publication59fd and independent goal882 actual Git parents; formal98fa unchanged')
git('-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF)
commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit

fresh=[];minimum=shutil.disk_usage(W).free
for e in manifest+[{'path':'METADATA-MANIFEST.json','bytes':(dest/'METADATA-MANIFEST.json').stat().st_size,'mode':stat.S_IMODE((dest/'METADATA-MANIFEST.json').stat().st_mode),'sha256':sha(dest/'METADATA-MANIFEST.json')},{'path':'RESTORE.json','bytes':(dest/'RESTORE.json').stat().st_size,'mode':stat.S_IMODE((dest/'RESTORE.json').stat().st_mode),'sha256':sha(dest/'RESTORE.json')}]:
 data=fetch(commit,REL,e['path'],e);temp=S/'metadata-fresh-member';assert not temp.exists();temp.write_bytes(data);temp.chmod(e['mode']);assert sha(temp)==e['sha256'] and temp.stat().st_size==e['bytes'] and stat.S_IMODE(temp.stat().st_mode)==e['mode'];minimum=min(minimum,shutil.disk_usage(W).free);assert minimum>500*1024*1024;temp.unlink();fresh.append(e)
save(S/'INDEX-FRESH.json',{'status':'verified','fixed_index_commit':commit,'archive_path':REL,'metadata_manifest_sha256':sha(dest/'METADATA-MANIFEST.json'),'INDEX_sha256':sha(dest/'INDEX.json'),'MASTER_sha256':sha(S/'MASTER.json'),'files':fresh,'metadata_files_fresh_materialized_verified':len(fresh),'min_observed_free_bytes':minimum,'raw_all_fixed_members_already_actual_full_fresh_HTTPS_verified':True,'exact_raw_and_explicit_append_union':True,'all_original_paths_bytes_modes_retained':True})
save(S/'INDEX-PUBLISHED.json',{'fixed_index_commit':commit,'archive_path':REL,'INDEX_sha256':sha(dest/'INDEX.json'),'metadata_manifest_sha256':sha(dest/'METADATA-MANIFEST.json'),'fresh_receipt':str(S/'INDEX-FRESH.json'),'fresh_receipt_sha256':sha(S/'INDEX-FRESH.json'),'metadata_files':len(fresh),'source_actual_parents':[DOC,GOAL],'free_bytes':shutil.disk_usage(W).free});print((S/'INDEX-PUBLISHED.json').read_text(),flush=True)

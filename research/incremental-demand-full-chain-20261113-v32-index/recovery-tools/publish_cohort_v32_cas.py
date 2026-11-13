from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,tarfile,urllib.request
import zstandard
from v32_cas_restore import sha,verify_new_chunk
from v32_coalesce_verified import coalesce_verified
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-32';A=W/'zil-demand-archive';REF='refs/heads/archive/source-history'
plan=json.loads((S/'CAS-PLAN-bytes.json').read_text());master=json.loads((S/'MASTER.json').read_text());entries={e['path']:e for e in master['files']};source=json.loads((S/'SOURCE.json').read_text());reserve=500*1024*1024
def save(p,d):
 with p.open('x') as f:json.dump(d,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def action(args):
 p=subprocess.run(args,capture_output=True,text=True)
 with (S/'CAS-OPERATIONS.jsonl').open('a') as f:f.write(json.dumps({'argv':args,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,p.stderr;return p.stdout.strip()
def git(*args):return action(['git','-C',str(A),*args])
assert json.loads((S/'LEGACY-FRESH.json').read_text())['status']=='verified';assert not git('status','--porcelain');assert git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD')
# Fixed receipt of physical changes; preserve the complete pre/post journal, not just totals.
rel='research/incremental-demand-full-chain-20261113-v32-legacy-fresh';d=A/rel;d.mkdir()
for name in ['LEGACY-FRESH.json','CAS-COALESCENCE.jsonl','CAS-COALESCENCE-RESULT.json','PLAN-PUBLISHED.json']:shutil.copy2(S/name,d/name)
for name in ['v32_cas_restore.py','v32_coalesce_verified.py','publish_cohort_v32_cas.py']:shutil.copy2(Path('/tmp')/name,d/name)
git('add','--',rel);git('commit','-m','Preserve actual fresh V32 legacy object recovery and exact raw physical coalescence receipts');git('-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF);c=git('rev-parse','HEAD');readback=[]
for p in d.iterdir():
 b=urllib.request.urlopen(f'https://raw.githubusercontent.com/zil-lang/zil/{c}/{rel}/{p.name}',timeout=60).read();assert len(b)==p.stat().st_size and hashlib.sha256(b).hexdigest()==sha(p);readback.append({'path':rel+'/'+p.name,'sha256':sha(p),'bytes':len(b)})
save(S/'LEGACY-PUBLISHED.json',{'fixed_commit':c,'path':rel,'actual_fresh_files':readback})
pending=[o for o in plan['objects'] if o['storage']=='new-exact-object'];pending.sort(key=lambda o:(0 if '/frozen/sources/' in o['representative'] else 1,o['representative']));chunks=[]
while pending:
 assert not git('status','--porcelain');assert git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD')
 rawlimit=64*1024*1024;piece=[];raw=0
 for o in pending:
  if piece and raw+o['bytes']>rawlimit:break
  piece.append(o);raw+=o['bytes']
 assert shutil.disk_usage(W).free>reserve+2*raw+max(o['bytes'] for o in piece)
 tag=f'v32-cas-{len(chunks)+1:03d}';rel='research/incremental-demand-full-chain-20261113-'+tag;d=A/rel;d.mkdir();archive=d/'objects.tar.zst';print(json.dumps({'stage':'new-CAS-start','tag':tag,'objects':len(piece),'raw_unique_bytes':raw,'free':shutil.disk_usage(W).free}),flush=True)
 with archive.open('xb') as out:
  with zstandard.ZstdCompressor(level=3).stream_writer(out,closefd=False) as z:
   with tarfile.open(fileobj=z,mode='w|') as tf:
    for o in piece:
     p=W/o['representative'];e=entries[o['representative']];st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_size==o['bytes'] and stat.S_IMODE(st.st_mode)==e['mode'] and sha(p)==o['sha256'];info=tarfile.TarInfo('objects/'+o['sha256']);info.size=o['bytes'];info.mode=e['mode'];info.mtime=0
     with p.open('rb') as f:tf.addfile(info,f)
 manifest={'schema':'exact-byte-object-tar-chunk-v1','master_sha256':sha(S/'MASTER.json'),'objects':[{'sha256':o['sha256'],'bytes':o['bytes'],'member':'objects/'+o['sha256']} for o in piece],'archive':{'name':archive.name,'bytes':archive.stat().st_size,'sha256':sha(archive)},'logical_modes':'per-path MASTER mode, independently applied only to own reconstructed files','acceptance':'artifact custody only; formal98fa/0db; firstd662 timer failure and all negative results retained'};save(d/'CHUNK.json',manifest)
 git('add','--',rel);git('commit','-m','Preserve V32 unique exact byte CAS objects '+tag);git('-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF);commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit
 proof=verify_new_chunk(commit,rel,sha(d/'CHUNK.json'),piece,entries,S/(tag+'-fresh'));save(S/(tag+'-FRESH.json'),proof)
 coalescence=coalesce_verified(piece,entries,W,S/'NEW-CAS-COALESCENCE.jsonl',S/(tag+'-holds'));save(S/(tag+'-COALESCENCE.json'),coalescence)
 # Dematerialize only this exact freshly restored archive worktree copy, with index backup.
 assert sha(archive)==manifest['archive']['sha256'];idx=Path(git('rev-parse','--path-format=absolute','--git-path','index'));backup=S/(tag+'-index-before-skip');shutil.copy2(idx,backup);body=subprocess.check_output(['git','-C',str(A),'cat-file','blob',commit+':'+rel+'/'+archive.name]);assert len(body)==manifest['archive']['bytes'] and hashlib.sha256(body).hexdigest()==manifest['archive']['sha256'];git('update-index','--skip-worktree','--',rel+'/'+archive.name);archive.unlink()
 rec={'tag':tag,'fixed_commit':commit,'archive_path':rel,'manifest_sha256':sha(d/'CHUNK.json'),'archive':manifest['archive'],'objects':[o['sha256'] for o in piece],'fresh_receipt':str(S/(tag+'-FRESH.json')),'fresh_receipt_sha256':sha(S/(tag+'-FRESH.json')),'raw_unique_bytes':raw,'logical_paths':sum(len(o['logical_paths']) for o in piece),'coalescence':coalescence,'own_git_worktree_copy_dematerialization':{'index_backup':str(backup),'index_backup_sha256':sha(backup),'Git_objects_retained':True,'original_raw_paths_deleted':0}}
 chunks.append(rec);completed={o['sha256'] for o in piece};pending=[o for o in pending if o['sha256'] not in completed];(S/'CAS-CHUNKS.json').write_text(json.dumps({'master_sha256':sha(S/'MASTER.json'),'chunks':chunks,'pending_unique_objects':len(pending)},separators=(',',':'))+'\n');print(json.dumps({'stage':'new-CAS-fresh-closed','tag':tag,'fixed_commit':commit,'compressed_bytes':manifest['archive']['bytes'],'pending_objects':len(pending),'free_bytes':shutil.disk_usage(W).free}),flush=True)
assert sum(len(x['objects']) for x in chunks)==4500
covered={p for o in plan['objects'] for p in o['logical_paths']};assert covered==set(entries) and sum(len(o['logical_paths']) for o in plan['objects'])==len(entries)
save(S/'RAW-CAS-UNION.json',{'status':'every logical regular file actual fresh HTTPS bytes-size-mode verified','master_sha256':sha(S/'MASTER.json'),'logical_files':len(entries),'logical_bytes':master['logical_bytes'],'unique_objects':len(plan['objects']),'new_unique_objects':4500,'new_compressed_bytes':sum(x['archive']['bytes'] for x in chunks),'chunks':chunks,'legacy_fresh_receipt_sha256':sha(S/'LEGACY-FRESH.json'),'exact_union_no_gaps_no_duplicates':True,'all_original_paths_retained':True,'metadata_final_index_closure':'pending','compiler_test_native_measurement_executions':0});print((S/'RAW-CAS-UNION.json').read_text()[:800],flush=True)

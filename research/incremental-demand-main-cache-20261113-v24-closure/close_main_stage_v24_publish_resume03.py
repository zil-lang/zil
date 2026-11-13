#!/usr/bin/env python3
"""Preserve final index recovery proof after its publisher has completely exited."""
from pathlib import Path
import hashlib,json,os,shutil,stat,subprocess,time,urllib.request
W=Path('/workspace');R=W/'zil-push-main';A=W/'zil-demand-archive';S=R/'out/evidence/full-push-checkpoint-24'
REL='research/incremental-demand-main-cache-20261113-v24-closure';D=A/REL;REF='refs/heads/archive/source-history'
LOG=R/'out/evidence/archive-main-stage-v24-closure-03.operations.jsonl'
def sha(p):
    with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
    with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def git(*args):return subprocess.check_output(['git','-C',str(A),*args],text=True).strip()
def run(args,label,check=True):
    r=subprocess.run(args,capture_output=True,text=True)
    with LOG.open('a') as f:f.write(json.dumps({'label':label,'argv':args,'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr},separators=(',',':'))+'\n')
    if check:assert not r.returncode,(label,r.stderr)
    return r
done=R/'out/evidence/archive-main-stage-v24-index-03.exit';last=0
while not done.exists():
    if time.monotonic()-last>50:print('waiting for completed index publisher and fresh recovery',flush=True);last=time.monotonic()
    time.sleep(2)
assert done.read_text().strip()=='0'
published=json.loads((S/'PUBLISHED.json').read_text());assert published['status']=='complete'
assert not git('status','--porcelain') and not D.exists()
run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-closure-base')
if git('rev-parse','HEAD')!=git('rev-parse','refs/remotes/origin/archive/source-history'):
    run(['git','-C',str(A),'merge','--ff-only','refs/remotes/origin/archive/source-history'],'ff-closure-base')
run(['git','-C',str(A),'merge-base','--is-ancestor',published['fixed_index_commit'],'HEAD'],'check-fixed-index-ancestor')
D.mkdir();files=[]
originals=[S/'PUBLISHED.json',S/'v24-index-https-recovery-03.json',S/'v24-index-https-recovery-03.log',R/'out/evidence/archive-main-stage-v24-index-03.log',done,R/'out/evidence/archive-main-stage-v24-index-03.operations.jsonl',Path(__file__)]
for src in originals:
    st=src.lstat();assert stat.S_ISREG(st.st_mode)
    target=D/src.name;shutil.copyfile(src,target);target.chmod(stat.S_IMODE(st.st_mode))
    entry={'name':target.name,'bytes':st.st_size,'mode':stat.S_IMODE(st.st_mode),'sha256':sha(src),'original_path':str(src)}
    assert target.stat().st_size==entry['bytes'] and sha(target)==entry['sha256'];files.append(entry)
record={'schema':'main-stage-v24-final-index-publication-closure-v1','status':'index_and_all_shards_actual_fresh_HTTPS_verified','fixed_index_commit':published['fixed_index_commit'],'fixed_index_path':published['archive_path'],'index_sha256':published['index_sha256'],'master_sha256':published['master_sha256'],'master_files':published['master_files'],'master_bytes':published['master_bytes'],'union_unique_files':published['unique_union_files'],'shards':published['shards'],'retired_original_raw_files':published['retired_original_raw_files'],'retired_original_raw_bytes':published['retired_original_raw_bytes'],'files':files,'source_and_research_histories_anchored_without_main_merge':True,'main_merge_or_speed_acceptance_claimed':False,'new_compiler_or_native_executions':0,'boundary':'Initial one-block performance was negative and remains unapproved. This closure preserves the completed index publisher log and actual fresh recovery receipt; its own final readback receipt is necessarily subsequent and retained locally.'}
save(D/'CLOSURE.json',record)
(D/'README.md').write_text('# V24 final publication closure\n\nThe complete fixed index is '+published['fixed_index_commit']+' at '+published['archive_path']+'. CLOSURE.json preserves its actual HTTPS recovery proof, completed publisher output/exit/operations and exact receipt byte hashes/modes. All '+str(published['shards'])+' raw shards were actually restored over fresh HTTPS, with every regular byte/size/mode checked and exact master union verified. Source, logs, receipts, immutable compiler paths and Git objects remain. No main merge or speed acceptance is claimed; the initial one-block timing is negative.\n')
run(['git','-C',str(A),'add','--',REL],'add-final-closure');run(['git','-C',str(A),'commit','-m','Preserve completed V24 index HTTPS recovery proof and publisher logs'],'commit-final-closure')
for attempt in range(4):
    r=run(['git','-C',str(A),'-c','pack.threads=1','push','origin','HEAD:'+REF],'push-closure-'+str(attempt),False)
    if not r.returncode:break
    run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-concurrent-closure')
    run(['git','-C',str(A),'merge','--no-edit','refs/remotes/origin/archive/source-history'],'ordinary-merge-concurrent-closure')
else:raise RuntimeError('ordinary closure push failed')
commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit
fresh=Path('/tmp/zil-v24-closure-03-fresh');assert not fresh.exists();fresh.mkdir(mode=0o700)
verified=[]
for src in sorted(D.iterdir()):
    assert src.is_file() and not src.is_symlink();expected={'bytes':src.stat().st_size,'mode':stat.S_IMODE(src.stat().st_mode),'sha256':sha(src)}
    url='https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+REL+'/'+src.name
    body=urllib.request.urlopen(url,timeout=60).read();q=fresh/src.name
    with q.open('xb') as f:f.write(body)
    q.chmod(expected['mode']);assert len(body)==expected['bytes'] and sha(q)==expected['sha256'] and stat.S_IMODE(q.stat().st_mode)==expected['mode']
    verified.append({'url':url,**expected})
shutil.rmtree(fresh)
result={'status':'complete','final_archive_commit':commit,'final_archive_ref':REF,'closure_path':REL,'fixed_index_commit':published['fixed_index_commit'],'fixed_index_path':published['archive_path'],'master_sha256':published['master_sha256'],'master_files':published['master_files'],'master_bytes':published['master_bytes'],'unique_union_files':published['unique_union_files'],'shards':published['shards'],'retired_original_raw_files':published['retired_original_raw_files'],'retired_original_raw_bytes':published['retired_original_raw_bytes'],'final_closure_files_actual_fresh_HTTPS_verified':verified,'main_merge_or_speed_acceptance_claimed':False,'new_compiler_or_native_executions':0,'git_status_clean':not git('status','--porcelain'),'free_bytes':shutil.disk_usage(W).free}
save(S/'FINAL-PUBLISHED.json',result);print(json.dumps(result),flush=True)

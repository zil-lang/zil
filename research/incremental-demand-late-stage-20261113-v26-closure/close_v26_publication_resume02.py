from pathlib import Path
import hashlib,json,os,stat,shutil,subprocess,urllib.request,time
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-26';A=W/'zil-demand-archive';REL='research/incremental-demand-late-stage-20261113-v26-closure';D=A/REL;REF='refs/heads/archive/source-history';LOG=R/'out/evidence/close-v26-publication-02.operations.jsonl'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def git(*args):return subprocess.check_output(['git','-C',str(A),*args],text=True).strip()
def run(argv,label,check=True):
 r=subprocess.run(argv,capture_output=True,text=True)
 with LOG.open('a') as f:f.write(json.dumps({'label':label,'argv':argv,'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr},separators=(',',':'))+'\n')
 if check:assert not r.returncode,(label,r.stderr)
 return r
done=R/'out/evidence/publish-v26-metadata-02.exit';last=0
while not done.exists():
 if time.monotonic()-last>50:print('waiting for completed metadata publisher and fresh HTTPS recovery',flush=True);last=time.monotonic()
 time.sleep(2)
assert done.read_text().strip()=='0'
p=json.loads((S/'PUBLISHED.json').read_text());assert p['status']=='complete';assert not git('status','--porcelain') and not D.exists()
run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-closure-base')
if git('rev-parse','HEAD')!=git('rev-parse','refs/remotes/origin/archive/source-history'):run(['git','-C',str(A),'merge','--ff-only','refs/remotes/origin/archive/source-history'],'ff-closure-base')
run(['git','-C',str(A),'merge-base','--is-ancestor',p['fixed_index_commit'],'HEAD'],'verify-index-ancestor')
index=json.loads((A/p['archive_path']/'INDEX.json').read_text());freshgit=Path('/tmp/zil-v26-final-index-parents-02.git');assert not freshgit.exists();run(['git','init','--bare',str(freshgit)],'init-final-source-parent-proof')
remote_commits=[]
for commit in [p['fixed_index_commit'],index['root_delivery_source']]:
 run(['git','-C',str(freshgit),'-c','gc.auto=0','fetch','--filter=tree:0','--no-tags','--depth=1','https://github.com/zil-lang/zil.git',commit],'fresh-final-source-object-'+commit)
 raw=subprocess.check_output(['git','-C',str(freshgit),'cat-file','commit',commit]);assert hashlib.sha1(b'commit '+str(len(raw)).encode()+b'\0'+raw).hexdigest()==commit;assert raw==subprocess.check_output(['git','-C',str(A),'cat-file','commit',commit]);remote_commits.append({'commit':commit,'parents':[line[7:] for line in raw.decode().splitlines() if line.startswith('parent ')],'raw_commit_sha256':hashlib.sha256(raw).hexdigest(),'raw_commit_text':raw.decode()})
assert index['root_delivery_source'] in remote_commits[0]['parents']
D.mkdir();save(D/'FINAL-SOURCE-PARENTS.json',{'status':'verified','fresh_isolated_git_dir':str(freshgit),'no_local_alternates_or_original_objects_used':True,'commits':remote_commits,'compiled_source':index['root_compiled_source'],'delivery_does_not_rebind_existing_compiler_binary':True})
files=[]
for src in [S/'PUBLISHED.json',S/'v26-index-https-recovery-02.json',S/'v26-index-https-recovery-02.log',R/'out/evidence/publish-v26-metadata-02.log',done,R/'out/evidence/publish-v26-metadata-02.operations.jsonl',Path(__file__)]:
 assert src.is_file() and not src.is_symlink();q=D/src.name;shutil.copyfile(src,q);mode=stat.S_IMODE(src.stat().st_mode);q.chmod(mode);assert sha(q)==sha(src);files.append({'name':q.name,'original':str(src),'bytes':q.stat().st_size,'mode':mode,'sha256':sha(q)})
record={'schema':'late-stage-v26-final-publication-closure-v1','status':'all_raw_and_metadata_actual_fresh_HTTPS_verified','fixed_index_commit':p['fixed_index_commit'],'fixed_index_path':p['archive_path'],'master_sha256':p['master_sha256'],'master_files':p['master_files'],'master_bytes':p['master_bytes'],'union_unique_files':p['unique_union_files'],'raw_shards':p['raw_shards'],'index_sha256':p['index_sha256'],'files':files,'root_compiled_source':index['root_compiled_source'],'root_delivery_source':index['root_delivery_source'],'measurement_runner_source':index['measurement_runner_source'],'readonly_qualifier_file':index['readonly_qualifier_file'],'initial_failed_gate_source':index['failed_gate_source'],'delivered_component_source':index['delivered_component_source'],'archiver_original_paths_deleted':0,'root_worktree_ref_retirement':'Explicit separate late root receipt; not an archiver raw deletion','compiler_native_build_measurement_executions':0,'main_merge_or_speed_acceptance_claimed':False,'boundary':index['boundary']};save(D/'CLOSURE.json',record)
(D/'README.md').write_text('# V26 final publication closure\n\nComplete fixed index: '+p['fixed_index_commit']+' / '+p['archive_path']+'. This closure preserves actual fresh metadata recovery proof and the completed publisher output/exit/operation log. FINAL-SOURCE-PARENTS.json separately freshly fetches and hash-checks final index and docs commit objects; docs are not a compiler identity. All four raw shards and metadata actually restored over HTTPS with hash/size/mode checks. V26 O2 successful full ELF bytes are all present; the historical gap belongs only to prior Aff/V25 as the immutable label clarification explains. Source/main speed acceptance remains unclaimed.\n')
run(['git','-C',str(A),'add','--',REL],'add-final-closure');run(['git','-C',str(A),'commit','-m','Preserve V26 final metadata HTTPS proof, source-parent readback and publisher logs'],'commit-final-closure')
for attempt in range(4):
 r=run(['git','-C',str(A),'-c','pack.threads=1','push','origin','HEAD:'+REF],'push-final-closure-'+str(attempt),False)
 if not r.returncode:break
 run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-concurrent-closure');run(['git','-C',str(A),'merge','--no-edit','refs/remotes/origin/archive/source-history'],'ordinary-merge-concurrent-closure')
else:raise RuntimeError('ordinary closure push rejected')
commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit;fresh=Path('/tmp/zil-v26-closure-02-fresh');assert not fresh.exists();fresh.mkdir();verified=[]
for src in sorted(D.iterdir()):
 assert src.is_file() and not src.is_symlink();expected={'bytes':src.stat().st_size,'mode':stat.S_IMODE(src.stat().st_mode),'sha256':sha(src)};url='https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+REL+'/'+src.name;body=urllib.request.urlopen(url,timeout=60).read();q=fresh/src.name
 with q.open('xb') as f:f.write(body)
 q.chmod(expected['mode']);assert len(body)==expected['bytes'] and sha(q)==expected['sha256'] and stat.S_IMODE(q.stat().st_mode)==expected['mode'];verified.append({'url':url,**expected})
shutil.rmtree(fresh);assert not git('status','--porcelain');result={'status':'complete','final_archive_commit':commit,'archive_ref':REF,'closure_path':REL,'fixed_index_commit':p['fixed_index_commit'],'fixed_index_path':p['archive_path'],'master_sha256':p['master_sha256'],'master_files':p['master_files'],'master_bytes':p['master_bytes'],'unique_union_files':p['unique_union_files'],'raw_shards':p['raw_shards'],'late_receipts':p['late_receipt_original_paths'],'root_compiled_source':index['root_compiled_source'],'root_delivery_source':index['root_delivery_source'],'archiver_original_paths_deleted':0,'compiler_native_build_measurement_executions':0,'main_merge_or_speed_acceptance_claimed':False,'final_closure_files_actual_fresh_HTTPS_verified':verified,'git_status_clean':True,'free_bytes':shutil.disk_usage(W).free};save(S/'FINAL-PUBLISHED.json',result);print(json.dumps(result),flush=True)

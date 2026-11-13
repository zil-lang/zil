from pathlib import Path
import subprocess,json,gzip,shutil,hashlib,stat,os
W=Path('/workspace');R=W/'zil-push-main';G=W/'zil-demand-goal';S=R/'out/evidence/full-push-checkpoint-32';L=S/'late';DOC='59fd0df5cf98cbb0dd57cbbdfaa3a8fe8160d628';GOAL='8821e9c9d966649f67650afc741aca56b4c19b33';OLDGOAL='c108d17c36940cc16667027faa8ae31ae6f9f048'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(repo,*args):return subprocess.check_output(['git','-C',str(repo),*args])
assert not L.exists();L.mkdir();snapshots=[]
for name in ['README.md','cohort_window_analysis.py','full-push-compact-checkpoint.json','full-push-compact-checkpoint.md']:
 p=L/name;p.write_bytes(git(R,'show',DOC+':tools/research/incremental-demand/'+name));snapshots.append({'role':'exact formal98fa/tool0db publication document; no current worktree read','commit':DOC,'git_path':'tools/research/incremental-demand/'+name,'path':str(p.relative_to(W)),'bytes':p.stat().st_size,'mode':stat.S_IMODE(p.stat().st_mode),'sha256':sha(p)})
for commit,names,prefix in [(GOAL,['goal.md','records/compiler-incremental-demand-continuation.json'],'current-goal-'),(OLDGOAL,['records/compiler-incremental-demand-continuation.json'],'prior-full-goal-')]:
 for name in names:
  p=L/(prefix+Path(name).name);p.write_bytes(git(G,'show',commit+':'+name));snapshots.append({'role':'exact current compact continuation' if commit==GOAL else 'verbatim previous cumulative record, historical only','commit':commit,'git_path':name,'path':str(p.relative_to(W)),'bytes':p.stat().st_size,'mode':stat.S_IMODE(p.stat().st_mode),'sha256':sha(p)})
p=L/('publication-source-'+DOC+'.tar.gz');proc=subprocess.Popen(['git','-C',str(R),'archive','--format=tar','--prefix=docs-'+DOC+'/',DOC],stdout=subprocess.PIPE)
with p.open('xb') as f:
 with gzip.GzipFile(fileobj=f,mode='wb',compresslevel=1,mtime=0) as z:shutil.copyfileobj(proc.stdout,z,1024*1024)
assert proc.wait()==0;snapshots.append({'role':'complete exact tools-only publication source; production remains98fa; uncommitted new ClosedBody WIP excluded','commit':DOC,'tree':git(R,'rev-parse',DOC+'^{tree}').decode().strip(),'path':str(p.relative_to(W)),'bytes':p.stat().st_size,'mode':stat.S_IMODE(p.stat().st_mode),'sha256':sha(p)})
record={'schema':'cohort-v32-late-publication-source-v1','formal_compiled':'98fa664743c4a2d94b5dc1eba5b61c7c1867ca4c','formal_tools':'0db08422dde4de3c222b2c72c444c9f375476612','publication_source':DOC,'goal_source':GOAL,'old_full_goal_source':OLDGOAL,'snapshots':snapshots,'new_ClosedBody_current_worktree_uncommitted_content_included':False,'no_runtime_gate_rebinding':True,'frozen_raw_MASTER_unchanged':True}
with (S/'LATE.json').open('x') as f:json.dump(record,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps({'status':'frozen-late','snapshots':len(snapshots),'bytes':sum(x['bytes'] for x in snapshots)}),flush=True)

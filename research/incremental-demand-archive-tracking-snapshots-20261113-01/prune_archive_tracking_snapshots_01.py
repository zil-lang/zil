from pathlib import Path
import subprocess,json,re,hashlib,os,urllib.request,shutil
A=Path('/workspace/zil-demand-archive');R=Path('/workspace/zil-push-main');S=R/'out/evidence/archive-tracking-retirement-01';S.mkdir();REF='refs/heads/archive/source-history';REL='research/incremental-demand-archive-tracking-snapshots-20261113-01';D=A/REL;Q=REL+'-result';E=A/Q
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def act(a,label,inp=None,check=True):
 p=subprocess.run(a,input=inp,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'stdin':inp,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n')
 if check:assert not p.returncode,(label,p.stderr)
 return p
def pub(folder,rel,msg):
 act(['git','-C',str(A),'add','--',rel],'add-index');act(['git','-C',str(A),'commit','-m',msg],'commit-index');act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-small-index-push');fixed=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==fixed;fresh=[]
 for p in sorted(folder.iterdir()):
  u='https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+rel+'/'+p.name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();fresh.append({'url':u,'bytes':len(b),'sha256':sha(p)})
 return fixed,fresh
assert not git('status','--porcelain');base=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==base
pattern=re.compile(r'^refs/remotes/origin/archive-source-history-v(?:1[1-9]|2[0-3])(?:[a-z0-9-]*)$');heads={line.split()[1]:line.split()[0] for line in git('ls-remote','--heads','origin').splitlines()};items=[];blocked=[]
for row in git('for-each-ref','--format=%(refname) %(objectname)','refs/remotes/origin/').splitlines():
 ref,tip=row.split()
 if not pattern.fullmatch(ref):continue
 name=ref.removeprefix('refs/remotes/origin/');server='refs/heads/'+name;reachable=act(['git','-C',str(A),'merge-base','--is-ancestor',tip,base],'verify-original-snapshot-ancestor-'+name,check=False).returncode==0
 e={'local_remote_tracking_ref':ref,'fixed_commit':tip,'actual_server_ref':server,'actual_server_ref_exists':server in heads,'actual_ancestor_of_fixed_current_archive':reachable}
 if not reachable or server in heads:blocked.append(e);continue
 assert git('cat-file','-t',tip)=='commit';items.append(e)
assert items;D.mkdir();index={'status':'proved redundant local tracking snapshots; actual remote refs absent','fixed_archive_base':base,'scope':'Only manual local origin/archive-source-history-v11..v23 snapshots; canonical origin/archive/source-history, main and all local branches/worktrees protected.','origin_fetch_specs':git('config','--get-all','remote.origin.fetch').splitlines(),'server_head_count_observed':len(heads),'snapshots':items,'blocked_retained':blocked,'fixed_recovery_does_not_depend_on_snapshot_names':'Recovery entrypoints receive exact immutable40hex Git commit and use raw HTTPS URLs, snapshot refs are historical local fetch destinations only. All snapshot commits remain actual reachable ancestors of canonical archive/source-history after removal.','actual_remote_ref_deletions_planned':0,'local_tracking_snapshot_deletions_planned':len(items),'new_compiler_test_native_measurement_or_large_file_hash_pack':False,'Git_objects_gc_or_deletion':False};save(S/'INDEX.json',index);shutil.copy2(S/'INDEX.json',D/'INDEX.json');shutil.copy2(__file__,D/Path(__file__).name)
(D/'README.md').write_text('# Redundant local archive tracking snapshots\n\nINDEX.json preserves every local ref to exact immutable commit. Each commit exists and is an actual ancestor of the fixed canonical archive base. These remote branch names do not exist on the server. They were manual fetch destinations; origin.fetch currently maps only main, so ordinary remote-prune would not remove them. Fixed recovery receives exact commit IDs and HTTPS paths, never these local aliases. Only the listed local tracking aliases are removed with an atomic expected-old update-ref transaction after this index is pushed/read back. No remote ref or Git object, local branch/worktree or canonical archive ref is removed.\n')
fixed,fresh=pub(D,REL,'Preserve fixed commit index before pruning redundant local archive tracking snapshots');save(S/'PRESERVATION-FRESH.json',{'status':'verified','fixed_commit':fixed,'path':REL,'actual_fresh_HTTPS':fresh})
assert git('ls-remote','origin',REF).split()[0]==fixed
for e in items:assert git('rev-parse',e['local_remote_tracking_ref'])==e['fixed_commit']
transaction='start\n'+''.join('delete '+e['local_remote_tracking_ref']+' '+e['fixed_commit']+'\n' for e in items)+'prepare\ncommit\n';act(['git','-C',str(A),'update-ref','--stdin'],'atomic-expected-old-local-tracking-snapshot-prune',inp=transaction)
remaining={r.split()[0] for r in git('for-each-ref','--format=%(refname) %(objectname)','refs/remotes/origin/').splitlines()};assert all(e['local_remote_tracking_ref'] not in remaining for e in items)
for e in items:act(['git','-C',str(A),'merge-base','--is-ancestor',e['fixed_commit'],'HEAD'],'post-prune-still-reachable-'+e['fixed_commit'])
result={'status':'complete','fixed_preservation_commit':fixed,'preservation_path':REL,'actual_remote_refs_deleted':0,'local_remote_tracking_snapshots_deleted':len(items),'local_branches_deleted':0,'worktrees_removed':0,'blocked_tracking_refs_retained':blocked,'all_fixed_snapshot_commits_still_actual_archive_ancestors':True,'Git_objects_deleted':0,'fixed_recovery_mutable_ref_dependency':False,'main_goal_root_current_sources_targets_untouched':True,'new_build_test_native_measurement_or_heavy_IO':False};save(S/'result.json',result);E.mkdir()
for name in ['result.json','PRESERVATION-FRESH.json','operations.jsonl']:shutil.copy2(S/name,E/name)
final,fresh=pub(E,Q,'Record exact local archive snapshot pruning with all fixed recovery commits reachable');save(S/'PUBLISHED.json',{'status':'complete','fixed_result_commit':final,'result_path':Q,'fixed_preservation_commit':fixed,'preservation_path':REL,'local_remote_tracking_refs_deleted':len(items),'actual_remote_refs_deleted':0,'blocked':blocked,'actual_result_fresh_HTTPS':fresh,'QUIET':True});Path('/tmp/full-push-archive-tracking-retirement.json').write_text(json.dumps({**result,'fixed_result_commit':final,'fixed_result_path':Q},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)

from pathlib import Path
import subprocess,json,concurrent.futures,hashlib,time
R=Path('/workspace/zil-push-main');G=Path('/workspace/zil-demand-goal');S=R/'out/evidence/branch-retirement-04';S.mkdir(exist_ok=False);ROOT=subprocess.check_output(['git','-C',str(R),'rev-parse','HEAD'],text=True).strip()
def git(tree,*a):return subprocess.check_output(['git','-C',str(tree),*a],text=True).strip()
remote={row.split()[1]:row.split()[0] for row in git(R,'ls-remote','--heads','origin').splitlines()};work=[]
for block in git(R,'worktree','list','--porcelain').split('\n\n'):
 d={}
 for line in block.splitlines():
  k,_,v=line.partition(' ');d[k]=v or True
 work.append(d)
record_files=[p for p in (G/'records').glob('compiler*.json')]+[G/'goal.md'];texts={str(p):p.read_text() for p in record_files}
def one(d):
 tree=Path(d['worktree']);ref=d.get('branch');tip=d['HEAD'];tracked=git(tree,'status','--porcelain=v1','--untracked-files=all');ignored=git(tree,'ls-files','--others','--ignored','--exclude-standard','--directory','--no-empty-directory').splitlines();tasks=[]
 for p in sorted((tree/'.task').glob('*.md')):
  tasks.append({'path':str(p.relative_to(tree)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'text':p.read_text()[:18000]})
 unique=git(R,'log','--format=%H %s',ROOT+'..'+tip).splitlines();hits=[]
 for path,text in texts.items():
  for needle in [tip,tip[:9],tip[:7],ref.removeprefix('refs/heads/') if ref else '###NOREF###']:
   at=text.find(needle)
   if at>=0:hits.append({'path':path,'needle':needle,'excerpt':text[max(0,at-200):at+500]});break
 ancestor=subprocess.run(['git','-C',str(R),'merge-base','--is-ancestor',tip,ROOT]).returncode==0
 return {**d,'remote_tip':remote.get(ref),'tracked_untracked_status':tracked,'ignored_directory_entries':ignored,'tasks':tasks,'unique_commits_vs_root':unique,'tip_real_ancestor_of_root':ancestor,'source_record_hits':hits,'tip_subject':git(R,'show','-s','--format=%s',tip),'tip_changed_paths':git(R,'diff-tree','--no-commit-id','--name-only','-r',tip).splitlines()}
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:items=list(pool.map(one,work))
value={'schema':'obsolete-root-owned-worktrees-audit-v1','root_read':ROOT,'goal_read':git(G,'rev-parse','HEAD'),'archive_read':git('/workspace/zil-demand-archive','rev-parse','HEAD'),'remote_heads_before':remote,'worktrees':items,'created_ns':time.time_ns(),'policy':'Read-only inventory. Branch name alone is not ownership proof. Protect other owners, main/goal/current/original468/heldAff/NativeRegion. No ignored/untracked deletion without separate exact custody proof.'}
(S/'INVENTORY.json').write_text(json.dumps(value,indent=2)+'\n')
for d in items:
 print(json.dumps({'worktree':d['worktree'],'ref':d.get('branch'),'tip':d['HEAD'][:9],'remote_match':d['remote_tip']==d['HEAD'],'dirty':bool(d['tracked_untracked_status']),'ignored':d['ignored_directory_entries'],'unique_commits':len(d['unique_commits_vs_root']),'ancestor':d['tip_real_ancestor_of_root'],'tasks':[x['path'] for x in d['tasks']],'records':len(d['source_record_hits'])}))

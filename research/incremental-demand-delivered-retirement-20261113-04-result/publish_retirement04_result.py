from pathlib import Path
import json,hashlib,os,subprocess,shutil,stat,urllib.request
W=Path('/workspace');R=W/'zil-push-main';A=W/'zil-demand-archive';S=R/'out/evidence/branch-retirement-04';REL='research/incremental-demand-delivered-retirement-20261113-04-result';D=A/REL;REF='refs/heads/archive/source-history'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(*a):return subprocess.check_output(['git','-C',str(A),*a],text=True).strip()
def action(a):
 p=subprocess.run(a,capture_output=True,text=True)
 with (S/'PUBLISH-OPERATIONS.jsonl').open('a') as f:f.write(json.dumps({'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr},separators=(',',':'))+'\n')
 assert not p.returncode,p.stderr
assert (S/'controller-02.exit').read_text().strip()=='0';r=json.loads((S/'RESULT.json').read_text());assert r['status']=='complete';assert not git('status','--porcelain') and not D.exists()
action(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'])
if git('rev-parse','HEAD')!=git('rev-parse','refs/remotes/origin/archive/source-history'):action(['git','-C',str(A),'merge','--ff-only','refs/remotes/origin/archive/source-history'])
D.mkdir();members=[]
paths=[p for p in S.iterdir() if p.is_file() and p.name not in ['PUBLISH-OPERATIONS.jsonl','publish-01.log','publish-01.exit']]
paths += [Path(__file__),Path('/tmp/inventory_obsolete_branches_04.py'),Path('/tmp/enrich_branch_audit_04.py')]
for p in sorted(paths):
 q=D/p.name;shutil.copyfile(p,q);mode=stat.S_IMODE(p.stat().st_mode);q.chmod(mode);assert sha(q)==sha(p);members.append({'name':q.name,'bytes':q.stat().st_size,'mode':mode,'sha256':sha(q),'original':str(p)})
summary={'schema':'delivered-author-retirement04-published-summary-v1','status':'complete','fixed_preservation_commit':r['fixed_preservation_commit'],'preservation_path':r['preservation_path'],'remote_refs_deleted':r['remote_ref_delete_count'],'local_branches_deleted':r['local_branch_delete_count'],'worktrees_removed':r['worktree_remove_count'],'ignored_files_removed_after_fresh_proof':r['ignored_removed_files'],'ignored_bytes_removed_after_fresh_proof':r['ignored_removed_bytes'],'blocked':r['blocked'],'protected_trees':r['protected_trees'],'protected_refs':r['protected_refs'],'members':members,'scope':'Exact successful own delete lists, not remote before/after subtraction. Original Git tips and complete source/decisions/small ignored payload actual fresh HTTPS recovered first; all Git objects retained. Protected current/main/goal/original468/heldAff/NativeRegion/newcold untouched. No new build/test/measurement.'}
(D/'SUMMARY.json').write_text(json.dumps(summary,indent=2)+'\n');(D/'README.md').write_text('# Delivered component retirement04 result\n\nActual own cleanup: **'+str(r['remote_ref_delete_count'])+' remote refs, '+str(r['local_branch_delete_count'])+' local branches, '+str(r['worktree_remove_count'])+' worktrees**. Remote deletions were one expected-tip atomic transaction; local cleanup used ordinary worktree remove and branch -d. '+str(r['ignored_removed_files'])+' ignored files / '+str(r['ignored_removed_bytes'])+' bytes were deleted only after fresh remote complete hash/size/mode recovery and current inode/bytes/mode/SHA recheck.\n\nOriginal 50 source tips, complete source tar snapshots, integration mappings, unique conclusions/adapted deltas and small ignored payload are fixed at '+r['fixed_preservation_commit']+' / '+r['preservation_path']+'. RESULT.json/OPERATIONS.jsonl and fresh source/commit proof give the complete actual lists. The large ignored zil-demand out/target and its local worktree/branch remain. Current root/main/goal/original468/heldAff/NativeRegion/new cold worker are protected. Other owners\' concurrent ref changes are never counted as this cleanup. No compiler build, tests or measurement were performed by this task.\n')
action(['git','-C',str(A),'add','--',REL]);action(['git','-C',str(A),'commit','-m','Record actual delivered branch retirement counts, preserved proofs and protected/blocked entries'])
for attempt in range(4):
 p=subprocess.run(['git','-C',str(A),'-c','pack.threads=1','push','origin','HEAD:'+REF],capture_output=True,text=True)
 with (S/'PUBLISH-OPERATIONS.jsonl').open('a') as f:f.write(json.dumps({'push_attempt':attempt,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n')
 if not p.returncode:break
 action(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history']);action(['git','-C',str(A),'merge','--no-edit','refs/remotes/origin/archive/source-history'])
else:raise RuntimeError('ordinary result push failed')
commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit;verified=[]
for p in sorted(D.iterdir()):
 url='https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+REL+'/'+p.name;body=urllib.request.urlopen(url,timeout=60).read();assert len(body)==p.stat().st_size and hashlib.sha256(body).hexdigest()==sha(p);verified.append({'url':url,'bytes':len(body),'sha256':sha(p)})
assert not git('status','--porcelain');record={'status':'complete','final_archive_commit':commit,'archive_ref':REF,'result_path':REL,'fixed_preservation_commit':r['fixed_preservation_commit'],'remote_refs_deleted':r['remote_ref_delete_count'],'local_branches_deleted':r['local_branch_delete_count'],'worktrees_removed':r['worktree_remove_count'],'ignored_files_removed':r['ignored_removed_files'],'ignored_bytes_removed':r['ignored_removed_bytes'],'blocked':r['blocked'],'published_result_files_actual_fresh_HTTPS_verified':verified,'Git_objects_retained':True,'source_build_test_measurement_executions':0,'git_status_clean':True,'free_bytes':shutil.disk_usage(W).free};(S/'PUBLISHED.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)

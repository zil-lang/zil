from pathlib import Path
import subprocess,json,hashlib,os,stat,time,shutil,urllib.request
A=Path('/workspace/zil-demand-archive');R=Path('/workspace/zil-push-main');S=R/'out/evidence/branch-retirement-09';REF='refs/heads/archive/source-history';P='research/incremental-demand-old-route-retirement-20261113-09';D=A/P;Q=P+'-result';E=A/Q
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def action(a,label):
 p=subprocess.run(a,capture_output=True,text=True)
 with (S/'delete-operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert p.returncode==0,(label,p.stderr);return p
assert (R/'out/evidence/retire-old-routes-09-source-02.exit').read_text().strip()=='0'
proof=json.loads((S/'SOURCE-FRESH.json').read_text());assert proof['status']=='verified';fixed=proof['fixed_source_commit'];assert git('ls-remote','origin',REF).split()[0]==fixed and git('rev-parse','HEAD')==fixed and not git('status','--porcelain')
pre=json.loads((D/'PRECONDITIONS.json').read_text());items=pre['items'];known=pre['known_previously_fresh_ignored'];new=pre['new_small_ignored_preserved'];allfiles=known+new
started=time.time_ns();checked=[]
for item in allfiles:
 p=Path(item['path']);s=p.lstat();assert stat.S_ISREG(s.st_mode) and all({'bytes':s.st_size,'dev':s.st_dev,'inode':s.st_ino,'mtime_ns':s.st_mtime_ns,'mode':stat.S_IMODE(s.st_mode)}[k]==item[k] for k in ['bytes','dev','inode','mtime_ns','mode']);actual=sha(p);assert actual==item['sha256'];checked.append({**item,'actual_current_sha256':actual})
save(S/'CURRENT-BYTES-VERIFIED.json',{'status':'verified','source_fixed_commit':fixed,'all_ignored_files':len(checked),'all_ignored_bytes':sum(x['bytes'] for x in checked),'elapsed_seconds':(time.time_ns()-started)/1e9,'files':checked,'original_V24_V25_full_fresh_and_new_4_pyc_HTTPS_custody_checked':True})
active=[];trees=[x['tree'] for x in items if x['tree']]
for proc in Path('/proc').iterdir():
 if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
 try:
  targets=[]
  try:targets.append(os.readlink(proc/'cwd'))
  except (OSError,PermissionError):pass
  for fd in (proc/'fd').iterdir():
   try:targets.append(os.readlink(fd))
   except (OSError,PermissionError):pass
  for t in targets:
   if any(t==tree or t.startswith(tree+'/') for tree in trees):active.append({'pid':proc.name,'path':t})
 except (OSError,PermissionError):pass
assert not active,active
heads={x.split()[1]:x.split()[0] for x in git('ls-remote','--heads','origin',*[x['branch'] for x in items]).splitlines()}
dirs=[]
for x in items:
 assert heads['refs/heads/'+x['branch']]==x['tip'];action(['git','-C',str(A),'merge-base','--is-ancestor',x['tip'],fixed],'actual-source-ancestor-'+x['branch'])
 if x['tree']:
  tree=Path(x['tree']);assert subprocess.check_output(['git','-C',str(tree),'rev-parse','HEAD']).decode().strip()==x['tip'];assert not subprocess.check_output(['git','-C',str(tree),'status','--porcelain=v1','--untracked-files=all']).strip()
  ignored={os.fsdecode(q) for q in subprocess.check_output(['git','-C',str(tree),'ls-files','--others','--ignored','--exclude-standard','-z']).split(b'\0') if q};assert ignored=={str(Path(q['path']).relative_to(tree)) for q in allfiles if q['path'].startswith(str(tree)+'/')}
  scopes=subprocess.check_output(['git','-C',str(tree),'ls-files','--others','--ignored','--exclude-standard','--directory']).decode().splitlines()
  for scope in scopes:
   root=tree/scope
   if root.is_dir():
    for parent,c,n in os.walk(root):dirs.append({'path':parent,'mode':stat.S_IMODE(Path(parent).stat().st_mode)})
save(S/'DELETE-PRECONDITIONS.json',{'status':'all3 exact remote tips; clean tracked/untracked; all3211 ignored bytes fully current-hashed and fixed-custody-proven; no observed active tree users','remote_expected':heads,'observed_active_uses':active,'ignored_directories':dirs,'source_fresh_receipt_sha256':sha(S/'SOURCE-FRESH.json'),'current_bytes_receipt_sha256':sha(S/'CURRENT-BYTES-VERIFIED.json'),'current_root_source_target_untouched':True})
action(['git','-C',str(A),'push','--atomic','origin','--delete',*[x['branch'] for x in items]],'normal-atomic-three-remote-ref-deletions-no-force')
assert not git('ls-remote','--heads','origin',*[x['branch'] for x in items])
for x in items:
 if not x['tree']:continue
 tree=Path(x['tree'])
 for item in [q for q in allfiles if q['path'].startswith(str(tree)+'/')]:
  p=Path(item['path']);s=p.lstat();assert (s.st_dev,s.st_ino,s.st_size,stat.S_IMODE(s.st_mode),s.st_mtime_ns)==(item['dev'],item['inode'],item['bytes'],item['mode'],item['mtime_ns']);p.unlink()
  with (S/'ignored-unlink-journal.jsonl').open('a') as f:f.write(json.dumps({'path':item['path'],'bytes':item['bytes'],'mode':item['mode'],'sha256':item['sha256'],'after_all_current_hash_and_remote_fresh_proof':True})+'\n');f.flush();os.fsync(f.fileno())
 for d in sorted([q for q in dirs if q['path'].startswith(str(tree)+'/')],key=lambda x:-len(Path(x['path']).parts)):
  p=Path(d['path']);assert p.is_dir() and not p.is_symlink() and stat.S_IMODE(p.stat().st_mode)==d['mode'];p.rmdir()
 assert not subprocess.check_output(['git','-C',str(tree),'ls-files','--others','--ignored','--exclude-standard','--directory']).strip();action(['git','-C',str(A),'worktree','remove',str(tree)],'normal-worktree-remove-'+tree.name);action(['git','-C',str(A),'branch','-d',x['branch']],'normal-local-branch-d-'+x['branch'])
result={'status':'complete','source_fixed_commit':fixed,'source_path':P,'remote_refs_deleted':3,'local_branches_deleted':2,'worktrees_removed':2,'propagation_had_no_local_branch_or_tree':True,'all_original_tips_remain_actual_archive_ancestors':True,'items':items,'ignored_exact_current_hashed_and_fresh_proven_removed_files':len(allfiles),'ignored_exact_current_hashed_and_fresh_proven_removed_bytes':sum(x['bytes'] for x in allfiles),'new_small_ignored_files_archived':4,'unknown_or_unpreserved_ignored_lost_files':0,'all_Git_objects_preserved':True,'unvalidated_routes_abandoned_not_merged_or_accepted':True,'historical_ELF_gaps_preserved':True,'current_root_branch_tree_target_untouched':True,'reactive_query_graph_other_owner_untouched':True,'build_test_native_measurement_executions':0};save(S/'result.json',result)
E.mkdir()
for p in S.iterdir():
 if p.is_file():shutil.copy2(p,E/p.name)
for pattern in ['retire-old-routes-09-source-*.log','retire-old-routes-09-source-*.exit']:
 for p in (R/'out/evidence').glob(pattern):shutil.copy2(p,E/p.name)
for p in [Path('/tmp/retire_old_routes_09_source.py'),Path('/tmp/retire_old_routes_09_source_02.py'),Path(__file__)]:shutil.copy2(p,E/p.name)
(E/'README.md').write_text('# Three old route retirements\n\nExactly three remote refs and two local branches/worktrees were normally retired after all original tips became actual parents of the fixed source anchor, remote-only complete source trees/blobs passed full Git fsck, and every ignored byte passed current SHA256/size/mode plus existing V24/V25 fresh custody or newly archived4pyc HTTPS verification. No forced push/remove, no Git object removal, no root source/target changes, and no compiler/test/native/measurement.\n\nPropagation468 is integrated ancestry. Aff remains performance-unaccepted with original negative timing and historical overwritten-ELF gaps preserved; NativeRegion remains an unvalidated prototype with disabled admission and incomplete prerequisite closure. Both old routes are set aside, not merged into main. Original source, all extant evidence and explicit negative decisions remain recoverable through the source anchor and V24/V25 dependencies. Other owners and reactive-query-graph/a6e are untouched.\n')
action(['git','-C',str(A),'add','--',Q],'add-fixed-result');action(['git','-C',str(A),'commit','-m','Record actual normal retirement of three obsolete demand routes with exact evidence custody'],'commit-fixed-three-route-result');action(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-result-push');final=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==final;verified=[]
for p in E.iterdir():
 if not p.is_file():continue
 url='https://raw.githubusercontent.com/zil-lang/zil/'+final+'/'+Q+'/'+p.name;b=urllib.request.urlopen(url,timeout=60).read();assert len(b)==p.stat().st_size and hashlib.sha256(b).hexdigest()==sha(p);verified.append({'path':Q+'/'+p.name,'bytes':len(b),'sha256':sha(p),'url':url})
assert not git('status','--porcelain');save(S/'PUBLISHED.json',{'status':'complete','fixed_source_commit':fixed,'source_path':P,'fixed_result_commit':final,'result_path':Q,'remote_refs_deleted':3,'local_branches_deleted':2,'worktrees_removed':2,'fixed_result_files_actual_fresh_HTTPS':verified,'archive_status_clean':True,'no_build_test_native_measurement':True,'heavy_IO_quiet':True,'free_bytes':shutil.disk_usage('/workspace').free})
decision={'schema':'obsolete-demand-route-convergence-v1','status':'all three obsolete routes normally retired, actual source and negative decisions preserved; no old compatibility branches required','fixed_source_commit':fixed,'fixed_source_path':P,'fixed_result_commit':final,'fixed_result_path':Q,'remote_refs_deleted':3,'local_branches_deleted':2,'worktrees_removed':2,'originals':[{'ref':x['branch'],'tip':x['tip'],'decision':x['decision']} for x in items],'evidence':'V24/V25 complete fixed extant raw plus4newpyc; all3211 ignored current SHA256/size/mode checked before removal; historical overwritten full-ELF gaps remain explicit','main_cache_performance_accepted':False,'native_region_prototype_accepted_or_enabled':False,'current_root_production_source_target_changed':False,'reactive_query_graph_unknown_owner_untouched':True,'compiler_test_native_measurement_executions':0}
Path('/tmp/full-push-old-route-convergence-decision.json').write_text(json.dumps(decision,sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)

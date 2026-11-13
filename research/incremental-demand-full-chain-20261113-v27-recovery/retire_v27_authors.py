from pathlib import Path
import os,stat,json,hashlib,subprocess,time
W=Path('/workspace');R=W/'zil-push-main';A=W/'zil-demand-archive';S=R/'out/evidence/branch-retirement-07';V=R/'out/evidence/full-push-checkpoint-27';S.mkdir(exist_ok=False)
ITEMS=[('research/early-scalar-ssa','e74380577421e557a841a580fcd0c9aa96416f92',W/'zil-early-scalar-ssa'),('research/typed-stage-body','88807747b56dff8ab5972ad60eecec8a7d56e2a6',W/'zil-typed-stage-body'),('research/native-link-profile','17c22e1a530f7e271561577702deaef54642df3e',W/'zil-native-link-profile'),('research/retained-native-publication','37c4213022c7e7457de8fdcd3118737a4833c1d4',W/'zil-retained-native-publication'),('research/native-link-profile-tools','73a9f146faa372f8a8e276cea50c7b8fca570e43',W/'zil-native-link-profile-tools')]
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def git(t,*a):return subprocess.check_output(['git','-C',str(t),*a],text=True).strip()
def action(argv,label):
 p=subprocess.run(argv,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':argv,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr},separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,(label,p.stderr);return p
assert (R/'out/evidence/close-v27-publication-01.exit').read_text().strip()=='0';closed=json.loads((V/'FINAL-PUBLISHED.json').read_text());fixed=closed['final_archive_commit'];assert not git(A,'status','--porcelain')
heads={row.split()[1]:row.split()[0] for row in git(A,'ls-remote','--heads','origin').splitlines()};assert heads['refs/heads/archive/source-history']==fixed
master=json.loads((V/'MASTER.json').read_text());assert sha(V/'MASTER.json')==closed['master_sha256'];entries={e['path']:e for e in master['files']};raw=json.loads((V/'RAW-UNION.json').read_text());proof=json.loads((V/'FRESH-SOURCE-PROOF.json').read_text());assert proof['status']=='verified'
covered={}
for shard in raw['shards']:
 receipt=Path(shard['fresh_receipt']);assert sha(receipt)==shard['fresh_receipt_sha256'] and json.loads(receipt.read_text())['status']=='verified'
 for path in shard['regular_paths']:assert path not in covered;covered[path]=shard['fixed_archive_commit']
state=[];known=[];unknown=[];dirs=[]
for branch,tip,tree in ITEMS:
 ref='refs/heads/'+branch;assert heads.get(ref)==tip and git(A,'rev-parse',ref)==tip and git(tree,'rev-parse','HEAD')==tip
 assert not git(tree,'status','--porcelain=v1','--untracked-files=all');action(['git','-C',str(A),'merge-base','--is-ancestor',tip,fixed],'verify-actual-original-ancestor-'+branch)
 ignored=[os.fsdecode(q) for q in subprocess.check_output(['git','-C',str(tree),'ls-files','--others','--ignored','--exclude-standard','-z']).split(b'\0') if q]
 scopes=git(tree,'ls-files','--others','--ignored','--exclude-standard','--directory').splitlines();state.append({'branch':branch,'ref':ref,'tip':tip,'tree':str(tree),'ignored_files':ignored,'ignored_scopes':scopes})
 for scope in scopes:
  root=tree/scope
  if root.is_dir() and not root.is_symlink():
   for parent,children,names in os.walk(root,followlinks=False):
    p=Path(parent);dirs.append({'path':str(p),'mode':stat.S_IMODE(p.lstat().st_mode)})
 for rel in ignored:
  p=tree/rel;st=p.lstat();path=str(p.relative_to(W));item={'path':path,'tree':str(tree),'relative_to_tree':rel,'dev':st.st_dev,'inode':st.st_ino,'bytes':st.st_size,'mode':stat.S_IMODE(st.st_mode),'mtime_ns':st.st_mtime_ns}
  if stat.S_ISLNK(st.st_mode):item.update(kind='symlink',target=os.readlink(p),sha256=hashlib.sha256(os.readlink(p).encode()).hexdigest());unknown.append(item);continue
  assert stat.S_ISREG(st.st_mode),path;item.update(kind='regular',sha256=sha(p))
  e=entries.get(path)
  if e and path in covered and all(item[k]==e[k] for k in ['dev','inode','bytes','mode','mtime_ns','sha256']):item['fresh_fixed_commit']=covered[path];known.append(item)
  else:unknown.append(item)
uses=[]
for proc in Path('/proc').iterdir():
 if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
 try:
  targets=[]
  try:targets.append(os.readlink(proc/'cwd'))
  except (FileNotFoundError,PermissionError):pass
  for fd in (proc/'fd').iterdir():
   try:targets.append(os.readlink(fd))
   except (FileNotFoundError,PermissionError):pass
  for target in targets:
   if any(target==str(tree) or target.startswith(str(tree)+'/') for _,_,tree in ITEMS):uses.append({'pid':int(proc.name),'path':target})
 except (FileNotFoundError,PermissionError,ProcessLookupError):continue
assert not uses,uses
pre={'status':'proved-before-any-deletion','explicit_root_authorization':'Current user/root explicitly instructs immediate retirement of exactly these five delivered original author branches, no additional confirmation; unknown ignored bytes must be retained persistently.','fixed_V27_closure':fixed,'fixed_V27_index':closed['fixed_index_commit'],'master_sha256':sha(V/'MASTER.json'),'source_proof_sha256':sha(V/'FRESH-SOURCE-PROOF.json'),'worktrees':state,'known_exact_fresh_ignored':known,'unknown_ignored_to_retain':unknown,'ignored_directories':dirs,'observed_active_uses':uses,'protected':'Root full-push-native, main/goal/archive, Aff HOLD, NativeRegion unresolved prototype, other workers and compact-scalar common tree/ref untouched.','remote_before':heads};save(S/'PRECONDITIONS.json',pre)
# Normal non-forced remote deletion. Current frozen tips are rechecked immediately;
# receive-pack applies its advertised old-object CAS and all five updates atomically.
now={row.split()[1]:row.split()[0] for row in git(A,'ls-remote','--heads','origin',*[q['ref'] for q in state]).splitlines()}
assert all(now.get(q['ref'])==q['tip'] for q in state)
action(['git','-C',str(A),'push','--atomic','origin','--delete',*[q['branch'] for q in state]],'normal-atomic-five-remote-ref-deletions-no-force')
remaining={row.split()[1]:row.split()[0] for row in git(A,'ls-remote','--heads','origin').splitlines()};assert all(q['ref'] not in remaining for q in state)
moved=[];removed=[];deleted=[]
for q in state:
 tree=Path(q['tree']);assert git(tree,'rev-parse','HEAD')==q['tip'] and not git(tree,'status','--porcelain=v1','--untracked-files=all')
 for item in [e for e in unknown if e['tree']==str(tree)]:
  original=W/item['path'];st=original.lstat();assert (st.st_dev,st.st_ino,st.st_size,stat.S_IMODE(st.st_mode),st.st_mtime_ns)==(item['dev'],item['inode'],item['bytes'],item['mode'],item['mtime_ns']);target=S/'retained-unknown'/tree.name/item['relative_to_tree'];target.parent.mkdir(parents=True,exist_ok=True);original.rename(target);assert stat.S_IMODE(target.lstat().st_mode)==item['mode'];assert (sha(target) if item['kind']=='regular' else hashlib.sha256(os.readlink(target).encode()).hexdigest())==item['sha256'];moved.append({**item,'retained_path':str(target)})
 for item in [e for e in known if e['tree']==str(tree)]:
  original=W/item['path'];st=original.lstat();assert (st.st_dev,st.st_ino,st.st_size,stat.S_IMODE(st.st_mode),st.st_mtime_ns)==(item['dev'],item['inode'],item['bytes'],item['mode'],item['mtime_ns']) and sha(original)==item['sha256'];original.unlink();removed.append(item)
  with (S/'ignored-unlink-journal.jsonl').open('a') as f:f.write(json.dumps({'after_actual_fresh_V27_proof':True,**item},separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
 for d in sorted((d for d in dirs if d['path'].startswith(str(tree)+'/')),key=lambda d:-len(Path(d['path']).parts)):
  p=Path(d['path']);assert p.is_dir() and not p.is_symlink() and stat.S_IMODE(p.lstat().st_mode)==d['mode'];p.rmdir()
 assert not git(tree,'ls-files','--others','--ignored','--exclude-standard','--directory');action(['git','-C',str(A),'worktree','remove',str(tree)],'normal-worktree-remove-'+tree.name);action(['git','-C',str(A),'branch','-d',q['branch']],'normal-local-branch-d-'+q['branch']);deleted.append(q)
result={'schema':'v27-five-delivered-author-retirement-v1','status':'complete','fixed_V27_preservation_closure':fixed,'fixed_V27_index':closed['fixed_index_commit'],'source_proof_sha256':sha(V/'FRESH-SOURCE-PROOF.json'),'remote_refs_deleted':5,'local_branches_deleted':5,'worktrees_removed':5,'retired':deleted,'ignored_exact_fresh_removed_files':len(removed),'ignored_exact_fresh_removed_bytes':sum(e['bytes'] for e in removed),'unknown_ignored_retained_files':len(moved),'unknown_ignored_retained':moved,'all_Git_objects_preserved':True,'protected_paths_and_refs_touched':False,'build_test_native_measurement_executions':0,'main_merge_or_performance_acceptance_claimed':False,'scope':'Only the five explicitly authorized completed original author branches; original source, all raw and conclusions remain fixed V27. Current root/common schema, compact tree/ref, main/goal/archive/Aff/NativeRegion and other workers untouched.','finished_ns':time.time_ns()};save(S/'result.json',result);assert not git(A,'status','--porcelain');print(json.dumps(result),flush=True)

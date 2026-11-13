from pathlib import Path
import subprocess,json,hashlib,os,shutil,urllib.request,time
A=Path('/workspace/zil-demand-archive');R=Path('/workspace/zil-push-main');V=R/'out/evidence/full-push-checkpoint-27';S=R/'out/evidence/branch-retirement-09';assert S.is_dir()
REF='refs/heads/archive/source-history'
P='research/incremental-demand-old-route-retirement-20261113-09';D=A/P
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def run(argv,label,check=True):
 p=subprocess.run(argv,capture_output=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':argv,'exit':p.returncode,'stdout':p.stdout.decode(errors='replace'),'stderr':p.stderr.decode(errors='replace')})+'\n');f.flush();os.fsync(f.fileno())
 if check:assert p.returncode==0,(label,p.stderr)
 return p
def git(*args):return subprocess.check_output(['git','-C',str(A),*args]).decode().strip()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def fresh_files(commit,folder,rel):
 records=[]
 for p in sorted(folder.rglob('*')):
  if not p.is_file():continue
  name=rel+'/'+p.relative_to(folder).as_posix();u='https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();records.append({'path':name,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest(),'url':u})
 return records
def push():
 for n in range(4):
  p=run(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-archive-push-'+str(n),False)
  if not p.returncode:break
  run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-concurrent')
  run(['git','-C',str(A),'merge','--no-edit','refs/remotes/origin/archive/source-history'],'preserve-concurrent-archive')
 else:raise RuntimeError('archive push failed')
 tip=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==tip;return tip
ITEMS=[('research/incremental-demand-propagation','468702058d02e382b69e446266fdad4a64b530d4',None,'delivered ancestor of current root; no compatibility branch required'),('perf/main-cache-serialization','aff75567688d493cbd3d3f006f6952d6923bb6c0','/workspace/zil-main-cache-serialization','HOLD, performance risk unresolved; route set aside, NOT main-ready or performance accepted'),('research/incremental-demand-native-region','998b8ae571da26fdfb68963e6b1a200dfa7bb77b','/workspace/zil-native-region','unvalidated failed/pending prototype; admission disabled, route set aside, NOT accepted or merged into main')]
assert not git('status','--porcelain');heads={x.split()[1]:x.split()[0] for x in git('ls-remote','--heads','origin').splitlines()};assert heads[REF]==git('rev-parse','HEAD');assert D.is_dir() and not list(D.iterdir())
entries={};custody={};dependencies=[]
for n in [24,25]:
 base=R/f'out/evidence/full-push-checkpoint-{n}';m=json.loads((base/'MASTER.json').read_text());shards=json.loads((base/'SHARDS.json').read_text());assert sha(base/'MASTER.json')==shards['master_sha256'];dependencies.append({'checkpoint':n,'master_sha256':sha(base/'MASTER.json'),'shards_receipt_sha256':sha(base/'SHARDS.json')})
 for e in m['files']:
  if e['path'].startswith('zil-main-cache-serialization/'):entries[e['path']]=e
 for q in shards['shards']:
  receipt=Path(q['fresh_receipt']);assert sha(receipt)==q['fresh_receipt_sha256'] and json.loads(receipt.read_text())['status']=='verified'
  for path in q['regular_paths']:
   if path.startswith('zil-main-cache-serialization/'):custody[path]={'checkpoint':n,'fixed_commit':q['fixed_archive_commit'],'archive_path':q['archive_path'],'fresh_receipt_sha256':q['fresh_receipt_sha256']}
state=[];ignored=[];new=[]
for branch,tip,tree,decision in ITEMS:
 assert heads.get('refs/heads/'+branch)==tip
 if tree:
  assert subprocess.check_output(['git','-C',tree,'rev-parse','HEAD']).decode().strip()==tip
  assert not subprocess.check_output(['git','-C',tree,'status','--porcelain=v1','--untracked-files=all']).strip()
  paths=[os.fsdecode(x) for x in subprocess.check_output(['git','-C',tree,'ls-files','--others','--ignored','--exclude-standard','-z']).split(b'\0') if x]
  for rel in paths:
   p=Path(tree)/rel;s=p.lstat();assert p.is_file() and not p.is_symlink();key=str(p.relative_to('/workspace'));item={'path':str(p),'workspace_path':key,'bytes':s.st_size,'dev':s.st_dev,'inode':s.st_ino,'mtime_ns':s.st_mtime_ns,'mode':s.st_mode&0o777}
   if key in entries:
    e=entries[key];assert key in custody and all(item[k]==e[k] for k in ['bytes','dev','inode','mtime_ns','mode']);item.update(sha256=e['sha256'],custody=custody[key]);ignored.append(item)
   else:
    assert '__pycache__/' in rel and p.suffix=='.pyc';item['sha256']=sha(p);dest=D/'new-ignored'/key;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,dest);assert sha(dest)==item['sha256'];item['new_fixed_path']=str(dest.relative_to(A));new.append(item)
 else:
  assert run(['git','-C',str(A),'show-ref','--verify','--quiet','refs/heads/'+branch],'assert-propagation-no-local-ref',False).returncode==1
 state.append({'branch':branch,'tip':tip,'tree':tree,'decision':decision,'owner_evidence':'Explicit current root/user owns this incremental-demand/full-push route, confirms no active implementer and authorizes source/evidence preservation then retirement. Original commit authors and source are captured; no claim inferred from branch name.'})
 (D/(tip+'.commit')).write_bytes(subprocess.check_output(['git','-C',str(A),'cat-file','commit',tip]))
 (D/(tip+'.source-tree')).write_bytes(subprocess.check_output(['git','-C',str(A),'ls-tree','-r','-l','-z',tip]))
assert len(new)==4 and sum(x['bytes'] for x in new)==67266
for name in ['full-push-main-cache-regression-astra-review.json','full-push-main-cache-retest-astra-review.json','full-push-main-cache-boundary-astra-review.json']:
 p=Path('/tmp')/name;shutil.copy2(p,D/name)
(D/'native-region-unique-source.diff').write_bytes(subprocess.check_output(['git','-C',str(A),'diff','--binary','86392ba1f2d52ee18f2f1c66d1b53bd8a35e9beb',ITEMS[2][1]]))
save(D/'PRECONDITIONS.json',{'status':'source/identity inventory complete; final current-byte SHA recheck pending before any deletion','items':state,'known_previously_fresh_ignored':ignored,'new_small_ignored_preserved':new,'dependencies':dependencies,'protected':'reactive-query-graph/a6e unknown owner untouched; root71c/current/full-push-native/target, main, goal, archives, other workers untouched','no_compile_test_measurement':True})
save(D/'DECISION.json',{'status':'retire obsolete branches after preserved exact evidence; no compatibility maintenance, NO performance acceptance','items':state,'main_cache_negative':'V24/V25 original raw and reviews remain authoritative. Source/functionality gates passing do not erase negative timing or unresolved repeated-performance risk. Historical overwritten full ELF gaps remain explicit.','native_region_negative':'Original source explicitly unvalidated and SDK allocation/emission prerequisites unresolved. Do not enable admission or merge prototype code into main. Preserve original source and decision, discontinue active branch.','current_implementation':'Only root integrated full-push branch; this operation changes no root production source or target.','original_468_source_ancestor_of_current_root_and_prior_archive':True})
run(['git','-C',str(A),'merge-base','--is-ancestor',ITEMS[0][1],'3e06887f44907a75df871025b78bd5e2756ce151'],'468-actual-prior-archive-ancestry')
run(['git','-C',str(A),'add','--',P],'add-three-route-original-source-decisions-and-new-pyc');old=git('rev-parse','HEAD');tree=git('write-tree');args=['commit-tree',tree,'-p',old]
for branch,tip,t,d in ITEMS:args+=['-p',tip]
commit=git(*args,'-m','Preserve three obsolete route tips and negative decisions before normal retirement');run(['git','-C',str(A),'update-ref',git('symbolic-ref','HEAD'),commit,old],'atomic-local-source-anchor');fixed=push();files=fresh_files(fixed,D,P)
F=Path('/tmp/zil-old-three-routes-source-fresh-09.git');assert not F.exists();run(['git','init','--bare',str(F)],'init-source-only-fresh');run(['git','-C',str(F),'-c','gc.auto=0','fetch','--depth=1','--no-tags','https://github.com/zil-lang/zil.git',*[x[1] for x in ITEMS]],'fresh-full-three-original-source-trees');run(['git','-C',str(F),'fsck','--full','--no-reflogs'],'fresh-all-three-source-object-integrity')
for b,tip,t,d in ITEMS:
 assert subprocess.check_output(['git','-C',str(F),'cat-file','commit',tip])==(D/(tip+'.commit')).read_bytes();assert subprocess.check_output(['git','-C',str(F),'ls-tree','-r','-l','-z',tip])==(D/(tip+'.source-tree')).read_bytes()
C=Path('/tmp/zil-old-three-routes-anchor-fresh-09.git');assert not C.exists();run(['git','init','--bare',str(C)],'init-anchor-commit-proof');run(['git','-C',str(C),'-c','gc.auto=0','fetch','--filter=tree:0','--depth=1','--no-tags','https://github.com/zil-lang/zil.git',fixed],'fresh-anchor-commit-only');raw=subprocess.check_output(['git','-C',str(C),'cat-file','commit',fixed]);parents=[x[7:] for x in raw.decode().splitlines() if x.startswith('parent ')];assert all(x[1] in parents for x in ITEMS)
save(S/'SOURCE-FRESH.json',{'status':'verified','fixed_source_commit':fixed,'path':P,'actual_parents':parents,'all_three_original_source_trees_and_blobs_fresh_fetched_and_fsck_verified':True,'source_fresh_git':str(F),'new_ignored_bytes_fresh_HTTPS_verified':67266,'files_actual_fresh_HTTPS':files})
shutil.copy2(D/'DECISION.json','/tmp/full-push-old-route-convergence-decision.json')
save(S/'READY-FOR-CURRENT-BYTE-RECHECK.json',{'status':'source custody complete; no deletion yet','fixed_source_commit':fixed,'path':P,'known_ignored_files':len(ignored),'known_ignored_bytes':sum(x['bytes'] for x in ignored),'new_small_ignored_files':4,'new_small_ignored_bytes':67266})
print((S/'READY-FOR-CURRENT-BYTE-RECHECK.json').read_text(),flush=True)

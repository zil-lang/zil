from pathlib import Path
import os,stat,json,hashlib,subprocess,gzip,shutil,time
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-32';F=S/'frozen'
def git(*a):return subprocess.check_output(['git','-C',str(R),*a],text=True).strip()
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
assert not S.exists();S.mkdir();F.mkdir();[(F/x).mkdir() for x in ['sources','scripts','reviews','obsolete-route']]
pins=[json.loads((R/f'out/runs/cohort-three-arm-0{i}-pins.json').read_text()) for i in [1,2]]
sources={'functional_and_first_timer_failure':pins[0]['candidate']['source'],'formal_compiled':pins[1]['candidate']['source'],'formal_measurement_tools':pins[1]['tool_source'],'obsolete_reactive_query_graph':'a6e6587dff31e72f5bb26f13dbf53b3162903243'}
boundary='V32 custody: formal98fa compiled d905/tool0db, complete02check+O2 each102rows,1080native/30wholeELF strict quality. d662 complete functional gates reused ONLY by independent timing-only source-equivalence receipt; first01check complete and01O2 first timer-child>parent hard failure retained, no accepted first01O2 comparison. No cross-window causal performance attribution. Full raw exact bytes retained; CAS uses only exact SHA/size/mode equality, never normalized bytes or timing substitution. New ClosedBody C++/Rust uncommitted successor excluded. Obsolete a6 source and original decision preserved, historical native/raw missing boundary explicit; source custody does not reconstruct absent historical raw or claim route acceptance. Prior SDK18 stock23/native-link recovered by fixed V23/V27 dependencies, no duplicate current SDK scan.'
source={'schema':'cohort-v32-source-v1','git_sources':sources,'root_source':sources['formal_compiled'],'measurement_tools_source':sources['formal_measurement_tools'],'formal_main_source':pins[1]['baseline']['source'],'pins':pins,'root_head_at_freeze':git('rev-parse','HEAD'),'root_status_at_freeze':git('status','--porcelain'),'active_excluded_path':'uncommitted next ClosedBody C++/Rust production WIP; no current worktree tracked content snapshotted','boundary':boundary,'source_exports':[],'snapshot_files':[],'prior_fixed_V31_recovery':'8ee98e6820158865ede7d68dd63e069f62ca076f','SDK18_and_native_link_fixed_recovery':'3e06887f44907a75df871025b78bd5e2756ce151'}
for commit in sorted(set(sources.values())):
 p=F/'sources'/(commit+'.tar.gz');proc=subprocess.Popen(['git','-C',str(R),'archive','--format=tar','--prefix='+commit+'/',commit],stdout=subprocess.PIPE)
 with p.open('xb') as f:
  with gzip.GzipFile(fileobj=f,mode='wb',compresslevel=1,mtime=0) as z:shutil.copyfileobj(proc.stdout,z,1024*1024)
 assert proc.wait()==0;source['source_exports'].append({'commit':commit,'tree':git('rev-parse',commit+'^{tree}'),'path':str(p.relative_to(W)),'bytes':p.stat().st_size,'sha256':sha(p)})
 assert shutil.disk_usage(W).free>500*1024*1024
for name in ['README.md','interruption-checkpoint-20261111.json','receipt.json']:
 body=subprocess.check_output(['git','-C',str(R),'show',sources['obsolete_reactive_query_graph']+':tools/research/reactive-query-graph/'+name]);(F/'obsolete-route'/name).write_bytes(body)
save(F/'obsolete-route/DECISION.json',{'original_ref':'research/reactive-query-graph','original_tip':sources['obsolete_reactive_query_graph'],'disposition':'obsolete route source custody; parent will retire only after durable source/parent proof','historical_native_and_raw':'Unavailable historical environment/native/raw are NOT restored by this source-only archive; original interruption checkpoint preserved verbatim. No fabricated old runtime acceptance.','current_WIP_included':False})
patterns=['native-cohort-*.py','prepare-cohort-window-01.py','full-push-cohort-window-analysis.py','full-push-native-cohort-*.json','full-push-cross-function-cohort-astra-design-review.json','full-push-other-artifact-storage-consolidation.json','full-push-immutable-blob-storage-consolidation.json','full-push-latest-runtime-dso-consolidation.json']
for pat in patterns:
 for p in sorted(Path('/tmp').glob(pat)):
  if not p.is_file() or p.is_symlink():continue
  q=F/('scripts' if p.suffix=='.py' else 'reviews')/p.name
  if q.exists():continue
  shutil.copy2(p,q);source['snapshot_files'].append({'original':str(p),'path':str(q.relative_to(W)),'bytes':q.stat().st_size,'mode':stat.S_IMODE(q.stat().st_mode),'sha256':sha(q)})
shutil.copy2(__file__,F/'scripts'/Path(__file__).name)
roots=[R/f'out/runs/cohort-three-arm-0{i}-{mode}' for i in [1,2] for mode in ['check','o2']]+[F]
roots += [R/'out/evidence'/n for n in ['raw-runtime-dso-hardlink-consolidation-01','latest-runtime-dso-hardlink-consolidation-02','immutable-blob-storage-consolidation-03','other-artifact-storage-consolidation-04']]
top=[]
for pat in ['cohort-*','native-cohort*','full-push-native-cohort*','thinlto-obsolete-branch-retirement-01.json','consolidate-*-storage-*.log','consolidate-*-storage-*.exit','consolidate-latest-runtime-dso-02.log','consolidate-latest-runtime-dso-02.exit']:
 for parent in [R/'out/runs',R/'out/evidence']:
  top.extend(p for p in parent.glob(pat) if p.is_file())
binaries=[Path(p['candidate']['binary']) for p in pins]
for p in pins:assert sha(Path(p['candidate']['binary']))==p['candidate']['binary_sha256']
files={};dirs={};links=[];special=[]
def add(p):
 rel=str(p.relative_to(W));st=p.lstat()
 if stat.S_ISDIR(st.st_mode):dirs[rel]={'path':rel,'mode':stat.S_IMODE(st.st_mode)};return
 if stat.S_ISLNK(st.st_mode):links.append({'path':rel,'target':os.readlink(p)});return
 if not stat.S_ISREG(st.st_mode):special.append({'path':rel,'mode':st.st_mode});return
 if rel in files:return
 h=sha(p);after=p.stat();assert (st.st_dev,st.st_ino,st.st_size,st.st_mode,st.st_mtime_ns)==(after.st_dev,after.st_ino,after.st_size,after.st_mode,after.st_mtime_ns),rel
 files[rel]={'path':rel,'bytes':st.st_size,'mode':stat.S_IMODE(st.st_mode),'sha256':h,'dev':st.st_dev,'inode':st.st_ino,'mtime_ns':st.st_mtime_ns,'retire_after_fresh':False}
for root in roots:
 assert root.is_dir(),root
 for parent,children,names in os.walk(root,followlinks=False):
  add(Path(parent));assert not any((Path(parent)/n).is_symlink() for n in children)
  for n in names:add(Path(parent)/n)
for p in top+binaries:add(p)
source['explicit_closed_roots']=[str(x.relative_to(W)) for x in roots];source['explicit_closed_top_files']=[str(x.relative_to(W)) for x in sorted(set(top))]
save(S/'SOURCE.json',source);add(S/'SOURCE.json');assert not links and not special
save(S/'STRUCTURE.json',{'directories':sorted(dirs.values(),key=lambda e:e['path']),'symlinks':links,'special_files':special});add(S/'STRUCTURE.json')
save(S/'MASTER.json',{'schema':'cohort-v32-exact-logical-CAS-master-v1','files':sorted(files.values(),key=lambda e:e['path']),'regular_files':len(files),'logical_bytes':sum(x['bytes'] for x in files.values()),'source_sha256':sha(S/'SOURCE.json'),'structure_sha256':sha(S/'STRUCTURE.json'),'boundary':boundary,'all_original_paths_retained':True})
save(S/'FREEZE.json',{'status':'frozen','master_sha256':sha(S/'MASTER.json'),'files':len(files),'bytes':sum(x['bytes'] for x in files.values()),'free_bytes':shutil.disk_usage(W).free});print((S/'FREEZE.json').read_text(),flush=True)

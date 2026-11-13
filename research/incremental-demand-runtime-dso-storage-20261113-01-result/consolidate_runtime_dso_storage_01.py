from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request,time
R=Path('/workspace/zil-push-main');A=Path('/workspace/zil-demand-archive');ASSESS=R/'out/evidence/raw-runtime-dso-hardlink-assessment-01';S=R/'out/evidence/raw-runtime-dso-hardlink-consolidation-01';S.mkdir();REF='refs/heads/archive/source-history';BASE='research/incremental-demand-runtime-dso-storage-20261113-01';D=A/BASE
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def st(p):
 s=p.lstat();return {k:getattr(s,k) for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_atime_ns','st_blocks','st_blksize']}
def stable(a,b):return all(a[k]==b[k] for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_blocks'])
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def journal(v):
 with (S/'journal.jsonl').open('a') as f:f.write(json.dumps(v,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label):
 p=subprocess.run(a,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n')
 assert not p.returncode,(label,p.stderr)
def publish(dest,rel,msg):
 act(['git','-C',str(A),'add','--',rel],'add-'+rel);act(['git','-C',str(A),'commit','-m',msg],'commit-'+rel);act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-small-push-'+rel);tip=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==tip;verified=[]
 for p in sorted(dest.iterdir()):
  if not p.is_file():continue
  url='https://raw.githubusercontent.com/zil-lang/zil/'+tip+'/'+rel+'/'+p.name;b=urllib.request.urlopen(url,timeout=60).read();assert b==p.read_bytes();verified.append({'url':url,'bytes':len(b),'sha256':sha(p)})
 return tip,verified
assessment=json.loads((ASSESS/'ASSESSMENT.json').read_text());old=json.loads((ASSESS/'CURRENT-STATS.json').read_text());assert assessment['eligible_old_files']==24 and len(assessment['groups'])==2 and not assessment['active_snapshot_uses'];groups=assessment['groups'];all_paths=[g['canonical_path'] for g in groups]+[p['path'] for g in groups for p in g['replacement_paths']];assert len(all_paths)==len(set(all_paths))==24;excluded={e['path'] for e in assessment['excluded']};assert len(excluded)==4;assert not excluded.intersection(all_paths);before_by_path={e['path']:e for e in old if e['path'] in all_paths};assert len(before_by_path)==24
for p in all_paths:
 q=Path(p);assert q.parent.name=='runtime-blobs' and q.parent.parent.parent==R/'out/runs';assert not q.parent.parent.name.startswith('producer-ssa-three-arm-01-');assert q.name==before_by_path[p]['custody']['sha256']+'.dso';assert q.is_file() and not q.is_symlink();assert stable(st(q),before_by_path[p]['fullstat'])
def active_uses():
 out=[]
 for proc in Path('/proc').iterdir():
  if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
  try:
   values=[]
   try:values.append((proc/'maps').read_text(errors='replace'))
   except OSError:pass
   for fd in (proc/'fd').iterdir():
    try:values.append(os.readlink(fd))
    except OSError:pass
   for path in all_paths:
    if any(path in v for v in values):out.append({'pid':proc.name,'path':path})
  except OSError:pass
 return out
assert not active_uses();sdk_before=[]
for e in assessment['protected_current_SDK_inodes']:
 p=Path(e['path']);s=p.stat();assert (s.st_dev,s.st_ino)==(e['dev'],e['inode']);sdk_before.append({'path':str(p),'stat':st(p)})
fresh_before=[]
for g in groups:
 for path in [g['canonical_path']]+[e['path'] for e in g['replacement_paths']]:
  p=Path(path);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==g['sha256'] and b['st_size']==g['bytes'] and stat.S_IMODE(b['st_mode'])==g['mode']==0o600 and b['st_dev']==g['dev'];assert not any((b['st_dev'],b['st_ino'])==(e['stat']['st_dev'],e['stat']['st_ino']) for e in sdk_before);fresh_before.append({'path':path,'sha256':h,'fullstat_before_hash':a,'fullstat_after_hash':b,'fixed_custody':before_by_path[path]['custody']})
pre={'status':'all24 actual current SHA/size/mode/exact stable metadata verified before any storage mutation','authorization':'Root explicitly authorizes exactly the assessed24 old paths/two groups,22 duplicate inodes; all logical paths/bytes/mode0600 remain. Latest V30 four paths and SDK/target/live publication protected. No content writes/chmod.','groups':groups,'original_assessment_sha256':sha(ASSESS/'ASSESSMENT.json'),'current_before':fresh_before,'protected_excluded_latest_V30':sorted(excluded),'protected_current_SDK_stats':sdk_before,'active_uses':active_uses(),'transaction':'Each group holds original duplicate inodes via temporary hardlinks under this receipt directory until all replacement paths verify; same-device temp link plus atomic rename. On any detected race stop that group, rollback paths from holds, preserve receipts.','no_new_native_build_test_measurement':True};assert not pre['active_uses'];save(S/'PRECONDITIONS.json',pre)
assert not git('status','--porcelain') and git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD');D.mkdir();shutil.copy2(S/'PRECONDITIONS.json',D/'PRECONDITIONS.json');shutil.copy2(ASSESS/'ASSESSMENT.json',D/'ASSESSMENT.json');shutil.copy2(__file__,D/Path(__file__).name);fixed,initial_fresh=publish(D,BASE,'Preserve exact old raw snapshot stats and byte proof before storage-only hardlink consolidation');save(S/'PRESERVATION-FRESH.json',{'status':'verified','fixed_commit':fixed,'path':BASE,'actual_fresh_HTTPS':initial_fresh})
before_map={e['path']:e['fullstat_after_hash'] for e in fresh_before};free_before=shutil.disk_usage('/workspace').free;done=[];after_records=[];group_receipts=[]
for number,g in enumerate(groups):
 assert not active_uses();canonical=Path(g['canonical_path']);expected=before_map[str(canonical)];assert stable(st(canonical),expected);hold_dir=S/'original-inode-holds'/str(number);hold_dir.mkdir(parents=True);prepared=[];replaced=[]
 # All members must still match the whole-group precondition immediately.
 for e in g['replacement_paths']:assert stable(st(Path(e['path'])),before_map[e['path']])
 try:
  for i,e in enumerate(g['replacement_paths']):
   target=Path(e['path']);old_stat=st(target);assert stable(old_stat,before_map[str(target)]);assert stable(st(canonical),expected);assert not active_uses()
   # A hardlink hold preserves this exact original inode for group rollback.
   hold=hold_dir/str(i);os.link(target,hold,follow_symlinks=False);assert (st(hold)['st_dev'],st(hold)['st_ino'])==(old_stat['st_dev'],old_stat['st_ino']);prepared.append({'path':str(target),'hold':str(hold),'original':old_stat})
   held_stat=st(target);assert held_stat['st_nlink']==old_stat['st_nlink']+1;assert all(held_stat[k]==old_stat[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns','st_blocks'])
   temp=target.with_name('.'+target.name+'.storage-01.tmp');assert not temp.exists() and not temp.is_symlink();os.link(canonical,temp,follow_symlinks=False);linked=st(temp);can=st(canonical);assert linked['st_dev']==g['dev'] and linked['st_ino']==expected['st_ino'] and linked['st_mode']==expected['st_mode'] and linked['st_size']==g['bytes'];assert can['st_nlink']==expected['st_nlink']+1
   assert stable(st(target),held_stat);journal({'phase':'prepared-before-atomic-replace','canonical':str(canonical),'target':str(target),'original':old_stat,'hold':str(hold),'temporary_link':str(temp),'canonical_after_temp_link':can,'fixed_prior_stat_commit':fixed})
   os.replace(temp,target);after=st(target);assert after['st_ino']==expected['st_ino'] and after['st_dev']==g['dev'] and after['st_mode']==expected['st_mode'] and after['st_size']==g['bytes'];expected=st(canonical);replaced.append(prepared[-1]);journal({'phase':'atomic-replace-complete-original-held','path':str(target),'before':old_stat,'after':after,'original_hold':str(hold),'logical_path_bytes_mode_retained':True})
  # Rehash every logical member after its final physical inode is selected.
  paths=[str(canonical)]+[e['path'] for e in g['replacement_paths']];post=[]
  for path in paths:
   p=Path(path);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==g['sha256'] and b['st_ino']==expected['st_ino'] and b['st_dev']==g['dev'] and b['st_size']==g['bytes'] and stat.S_IMODE(b['st_mode'])==0o600;post.append({'path':path,'sha256':h,'fullstat_before_hash':a,'fullstat_after_hash':b})
  assert not active_uses()
  # Group commit: discard only this operation's rollback-hold aliases. Every
  # original logical evidence path still exists with the verified same bytes.
  for entry in prepared:
   hold=Path(entry['hold']);a=st(hold);orig=entry['original'];assert (a['st_dev'],a['st_ino'],a['st_size'],a['st_mode'])==(orig['st_dev'],orig['st_ino'],orig['st_size'],orig['st_mode']);hold.unlink();journal({'phase':'group-verified-retire-own-rollback-hold','held_alias':str(hold),'original_logical_path_retained':entry['path'],'original_inode_historical':orig['st_ino']})
  hold_dir.rmdir();after_records+=post;done+=replaced;group_receipts.append({'status':'complete','sha256':g['sha256'],'mode':g['mode'],'bytes':g['bytes'],'canonical':str(canonical),'canonical_inode_preserved':expected['st_ino'],'logical_paths':len(paths),'duplicate_independent_inodes_consolidated':len(replaced),'post_all_paths_rehashed':True,'new_nlink':st(canonical)['st_nlink']});journal({'phase':'group-complete','sha256':g['sha256'],'paths':paths,'free_bytes':shutil.disk_usage('/workspace').free})
 except BaseException as error:
  failures=[]
  for entry in reversed(replaced):
   try:os.replace(entry['hold'],entry['path']);journal({'phase':'group-rollback-original-inode-restored','path':entry['path'],'inode':entry['original']['st_ino']})
   except BaseException as e:failures.append({'path':entry['path'],'error':repr(e)})
  save(S/('GROUP-'+str(number)+'-STOPPED.json'),{'status':'stopped on race/error; no further group action','error':repr(error),'rollback_failures':failures,'prepared':prepared,'replaced':replaced});raise
assert len(done)==22 and len(after_records)==24
for e in sdk_before:assert st(Path(e['path']))==e['stat']
for path in excluded:assert st(Path(path))==next(e['fullstat'] for e in old if e['path']==path)
result={'status':'complete storage-only consolidation; no evidence content or logical path deleted','groups':group_receipts,'logical_old_paths_retained':24,'duplicate_independent_inodes_consolidated':22,'canonical_inodes_preserved':2,'before_current_SHA_and_fullstat':fresh_before,'after_current_SHA_and_fullstat':after_records,'prior_original_physical_metadata_preserved_in_fixed_receipt':fixed,'physical_inode_nlink_ctime_mtime_changes_are_intentional_and_reported':True,'original_logical_path_bytes_mode0600_preserved':True,'protected_latest_V30_four_paths_all_fullstat_unchanged':True,'protected_current_SDK_fullstats_unchanged':True,'no_target_SDK_or_other_raw_file_linked_or_chmodded':True,'free_bytes_before':free_before,'free_bytes_after':shutil.disk_usage('/workspace').free,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'new_build_test_native_measurement_executions':0};save(S/'result.json',result);rel=BASE+'-result';dest=A/rel;dest.mkdir()
for name in ['result.json','journal.jsonl','PRESERVATION-FRESH.json','operations.jsonl']:shutil.copy2(S/name,dest/name)
(dest/'README.md').write_text('# Exact old raw runtime snapshot storage consolidation\n\nAll24logical old paths retain complete bytes and mode0600, each rehashed before and after. Two pre-existing old raw canonicals retain their original inode;22duplicate physical inodes intentionally become hardlinks to their equal canonical. Original inode/nlink/mtime/ctime/fullstat remain in the fixed precondition and result history, not falsely described as unchanged current identity. Each group held original inodes through complete post-replacement verification, then removed only its own rollback aliases. Current SDK/target and four newestV30 paths were excluded. No raw content/path, source, executable, test or measurement result was deleted/rewritten. This is storage equality, never a new timing/gate acceptance.\n')
final,fresh=publish(dest,rel,'Record verified storage-only raw DSO hardlinks with all evidence paths and bytes retained');assert not git('status','--porcelain');save(S/'PUBLISHED.json',{'status':'complete','fixed_preconditions_commit':fixed,'preconditions_path':BASE,'fixed_result_commit':final,'result_path':rel,'logical_paths_preserved':24,'duplicate_physical_inodes_consolidated':22,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'free_bytes':shutil.disk_usage('/workspace').free,'result_actual_fresh_HTTPS':fresh,'QUIET':True});Path('/tmp/full-push-runtime-dso-storage-consolidation.json').write_text(json.dumps({k:v for k,v in json.loads((S/'PUBLISHED.json').read_text()).items() if k!='result_actual_fresh_HTTPS'},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)

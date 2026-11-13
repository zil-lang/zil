from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request
R=Path('/workspace/zil-push-main');A=Path('/workspace/zil-demand-archive');ASSESS=R/'out/evidence/latest-runtime-dso-hardlink-assessment-02';S=R/'out/evidence/latest-runtime-dso-hardlink-consolidation-02';S.mkdir();REF='refs/heads/archive/source-history';BASE='research/incremental-demand-runtime-dso-storage-20261113-02';D=A/BASE
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def st(p):
 s=p.lstat();return {k:getattr(s,k) for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_atime_ns','st_blocks','st_blksize']}
def stable(a,b):return all(a[k]==b[k] for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_blocks'])
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def note(v):
 with (S/'journal.jsonl').open('a') as f:f.write(json.dumps(v,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label):
 a=['git','-c','gc.auto=0','-c','maintenance.auto=false']+a[1:] if a[0]=='git' else a;p=subprocess.run(a,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n')
 assert not p.returncode,(label,p.stderr)
def publish(d,rel,msg):
 act(['git','-C',str(A),'add','--',rel],'add');act(['git','-C',str(A),'commit','-m',msg],'commit');act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-push');fixed=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==fixed;fresh=[]
 for p in d.iterdir():
  u='https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+rel+'/'+p.name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();fresh.append({'url':u,'bytes':len(b),'sha256':sha(p)})
 return fixed,fresh
assessment=json.loads((ASSESS/'ASSESSMENT.json').read_text());assert assessment['eligible_files']==8 and not assessment['excluded'] and not assessment['active_snapshot_uses'];rows=assessment['observed_paths'];assert len(rows)==8;paths=[e['path'] for e in rows];canonicals=sorted({e['canonical'] for e in rows});assert len(canonicals)==2
def active():
 found=[]
 for p in Path('/proc').iterdir():
  if not p.name.isdigit() or int(p.name)==os.getpid():continue
  try:
   values=[]
   try:values.append((p/'maps').read_text(errors='replace'))
   except OSError:pass
   for fd in (p/'fd').iterdir():
    try:values.append(os.readlink(fd))
    except OSError:pass
   for path in paths+canonicals:
    if any(path in v for v in values):found.append({'pid':p.name,'path':path})
  except OSError:pass
 return found
assert not active();before=[]
for e in rows:
 p=Path(e['path']);assert p.resolve()==p and p.parent.name=='runtime-blobs' and p.parent.parent.parent==R/'out/runs';assert p.parent.parent.name.startswith(('producer-ssa-three-arm-01-','owned-front-metadata-three-arm-01-'));assert stable(st(p),e['fullstat']);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==e['sha256'] and stat.S_IMODE(b['st_mode'])==e['mode']==0o600 and b['st_size']==e['bytes'] and b['st_nlink']==1;before.append({**e,'sha256_rechecked':h,'before_hash':a,'after_hash':b})
canonical_before=[]
for path in canonicals:
 e=next(e for e in rows if e['canonical']==path);p=Path(path);assert p.resolve()==p and stable(st(p),e['canonical_fullstat']);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==e['sha256'] and b['st_dev']==e['fullstat']['st_dev'] and b['st_nlink']==12 and stat.S_IMODE(b['st_mode'])==0o600;canonical_before.append({'path':path,'sha256':h,'before_hash':a,'after_hash':b})
sdk=[]
for path in ['/workspace/.zil-environment/llvm-function-products-v18/lib/libLLVM.so.23.1','/workspace/.zil-environment/llvm-23.1.4/lib/libLLVM.so.23.1']:
 p=Path(path);sdk.append({'path':path,'fullstat':st(p)});assert all((e['after_hash']['st_dev'],e['after_hash']['st_ino'])!=(sdk[-1]['fullstat']['st_dev'],sdk[-1]['fullstat']['st_ino']) for e in canonical_before)
save(S/'PRECONDITIONS.json',{'status':'exact8 latest closed/fixed/fresh paths plus2oldcanonical actual SHA/stat rechecked','rows':before,'canonicals':canonical_before,'protected_SDK':sdk,'scope':'Only8new raw paths atomically acquire equal old canonical inode; old24path data/mode remain, link-count/ctime implications explicit. No SDK/target/source/Git/other raw data changes. Current code48e remains untested and is not V31formal536.','active_uses':active(),'assessment_sha256':sha(ASSESS/'ASSESSMENT.json')});assert not active();assert not git('status','--porcelain') and git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD');D.mkdir();shutil.copy2(S/'PRECONDITIONS.json',D/'PRECONDITIONS.json');shutil.copy2(ASSESS/'ASSESSMENT.json',D/'ASSESSMENT.json');shutil.copy2(__file__,D/Path(__file__).name);fixed,fresh=publish(D,BASE,'Preserve exact latest eight snapshot byte/stat proof before storage-only continuation');save(S/'PRESERVATION-FRESH.json',{'fixed_commit':fixed,'path':BASE,'actual_fresh_HTTPS':fresh});free_before=shutil.disk_usage('/workspace').free;groups=[];after=[]
for number,path in enumerate(canonicals):
 canonical=Path(path);can=next(e for e in canonical_before if e['path']==path);expected=can['after_hash'];members=[e for e in before if e['canonical']==path];assert len(members)==4 and stable(st(canonical),expected) and not active();holds=S/'holds'/str(number);holds.mkdir(parents=True);replaced=[]
 for e in members:assert stable(st(Path(e['path'])),e['after_hash'])
 try:
  for i,e in enumerate(members):
   target=Path(e['path']);orig=st(target);assert stable(orig,e['after_hash']) and stable(st(canonical),expected) and not active();hold=holds/str(i);os.link(target,hold,follow_symlinks=False);held=st(target);assert held['st_nlink']==2 and all(held[k]==orig[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns','st_blocks']);temp=target.with_name('.'+target.name+'.storage-02.tmp');assert not temp.exists();os.link(canonical,temp,follow_symlinks=False);linked=st(temp);assert linked['st_ino']==expected['st_ino'] and linked['st_dev']==expected['st_dev'] and linked['st_mode']==expected['st_mode'] and linked['st_size']==expected['st_size'];assert stable(st(target),held);note({'phase':'prepared','path':str(target),'before':orig,'hold':str(hold),'temp':str(temp),'canonical':path});os.replace(temp,target);expected=st(canonical);assert st(target)['st_ino']==expected['st_ino'];replaced.append({'path':str(target),'hold':str(hold),'before':orig});note({'phase':'atomic-replace-complete-original-held','path':str(target),'before':orig,'after':st(target),'canonical':path})
  verified=[]
  for name in [path]+[e['path'] for e in members]:
   p=Path(name);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==can['sha256'] and b['st_ino']==can['after_hash']['st_ino'] and b['st_nlink']==16 and b['st_mode']==can['after_hash']['st_mode'];verified.append({'path':name,'sha256':h,'before_hash':a,'after_hash':b})
  assert not active()
  for e in replaced:
   hold=Path(e['hold']);h=st(hold);assert h['st_ino']==e['before']['st_ino'] and h['st_size']==e['before']['st_size'] and h['st_mode']==e['before']['st_mode'];hold.unlink();note({'phase':'group-verified-retire-own-hold','hold':str(hold),'logical_path_preserved':e['path'],'historical_old_inode':e['before']['st_ino']})
  holds.rmdir();after+=verified;groups.append({'sha256':can['sha256'],'canonical':path,'canonical_inode_preserved':can['after_hash']['st_ino'],'canonical_nlink_before':12,'canonical_nlink_after':16,'new_paths_consolidated':4,'all_new_and_canonical_paths_rehashed_after':True,'existing_old_alias_data_mode_unchanged_link_count_ctime_follow_shared_inode':True});note({'phase':'group-complete','canonical':path,'new_paths':4,'free_bytes':shutil.disk_usage('/workspace').free})
 except BaseException as ex:
  errors=[]
  for e in reversed(replaced):
   try:os.replace(e['hold'],e['path'])
   except BaseException as fail:errors.append({'path':e['path'],'error':repr(fail)})
  save(S/('GROUP-'+str(number)+'-STOPPED.json'),{'status':'group stopped and rollback attempted, no further group action','error':repr(ex),'rollback_errors':errors,'replaced':replaced});raise
for e in sdk:assert st(Path(e['path']))==e['fullstat']
result={'status':'complete exact8path storage continuation; all evidence paths/bytes/mode retained','fixed_preconditions_commit':fixed,'preconditions_path':BASE,'groups':groups,'before':before,'canonical_before':canonical_before,'after':after,'new_logical_paths_retained':8,'old24_logical_paths_and_contents_retained':True,'duplicate_independent_inodes_consolidated':8,'physical_inode_mtime_ctime_nlink_changes_recorded_not_claimed_unchanged':True,'mode0600_preserved_without_chmod':True,'SDK_target_source_and_other_raw_not_modified':True,'free_bytes_before':free_before,'free_bytes_after':shutil.disk_usage('/workspace').free,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'new_build_test_native_measurement_executions':0};save(S/'result.json',result);rel=BASE+'-result';dest=A/rel;dest.mkdir()
for name in ['result.json','journal.jsonl','operations.jsonl','PRESERVATION-FRESH.json']:shutil.copy2(S/name,dest/name)
(dest/'README.md').write_text('# Latest eight immutable snapshot storage continuation\n\nExactly8closed/fixed/fresh V30/V31 raw DSO paths became atomic hardlinks to their byte-identical mode0600 old raw canonicals. Every source and canonical was SHA/stat rechecked before; all8pluscanonicals rehashed after. Group original-inode holds remained until full verification. Both canonical inodes remain; their nlink12->16/ctime implications also affect older aliases and are explicitly historical metadata, not a new runtime observation. All logical paths, complete content/mode and every old timing/gate/source-role remain. No SDK/target/source/current live publisher or other raw content/path was touched; no chmod or compiler/test/measurement executed.\n')
final,fresh=publish(dest,rel,'Record exact latest eight raw DSO storage links with all historical metadata retained');save(S/'PUBLISHED.json',{'status':'complete','fixed_result_commit':final,'result_path':rel,'fixed_preconditions_commit':fixed,'preconditions_path':BASE,'new_logical_paths_preserved':8,'duplicate_physical_inodes_consolidated':8,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'result_actual_fresh_HTTPS':fresh,'free_bytes':shutil.disk_usage('/workspace').free,'QUIET':True});Path('/tmp/full-push-latest-runtime-dso-consolidation.json').write_text(json.dumps({k:v for k,v in json.loads((S/'PUBLISHED.json').read_text()).items() if k!='result_actual_fresh_HTTPS'},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)

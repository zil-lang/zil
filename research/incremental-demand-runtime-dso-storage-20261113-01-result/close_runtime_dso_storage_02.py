from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request,time
R=Path('/workspace/zil-push-main');A=Path('/workspace/zil-demand-archive');ASSESS=R/'out/evidence/raw-runtime-dso-hardlink-assessment-01';S=R/'out/evidence/raw-runtime-dso-hardlink-consolidation-01';assert S.is_dir();REF='refs/heads/archive/source-history';BASE='research/incremental-demand-runtime-dso-storage-20261113-01';D=A/BASE
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
assessment=json.loads((ASSESS/'ASSESSMENT.json').read_text());old=json.loads((ASSESS/'CURRENT-STATS.json').read_text());pre=json.loads((S/'PRECONDITIONS.json').read_text());pr=json.loads((S/'PRESERVATION-FRESH.json').read_text());fixed=pr['fixed_commit'];groups=pre['groups'];fresh_before=pre['current_before'];sdk_before=pre['protected_current_SDK_stats'];excluded=pre['protected_excluded_latest_V30'];assert git('rev-parse','HEAD')==fixed and git('ls-remote','origin',REF).split()[0]==fixed and not git('status','--porcelain')
entries=[json.loads(line) for line in (S/'journal.jsonl').read_text().splitlines()];commits=[e for e in entries if e['phase']=='group-complete'];assert len(commits)==2 and {e['sha256'] for e in commits}=={e['sha256'] for e in groups};done=[e for e in entries if e['phase']=='atomic-replace-complete-original-held'];assert len(done)==22
assert not list((S/'original-inode-holds').rglob('*'))
protected_checks=[]
for path in excluded:
 now=st(Path(path));prior=next(e['fullstat'] for e in old if e['path']==path);differences={k:[v,now.get(k)] for k,v in prior.items() if now.get(k)!=v};assert not differences,(path,differences);protected_checks.append({'path':path,'all_original_fields_exactly_equal':True,'extra_current_fields':sorted(set(now)-set(prior)),'fullstat_current':now,'fullstat_assessment':prior})
for e in sdk_before:assert st(Path(e['path']))==e['stat']
all_paths=[e['path'] for e in fresh_before];after_records=[];group_receipts=[]
for g in groups:
 canonical=Path(g['canonical_path']);identity=st(canonical);assert identity['st_ino']==g['canonical_inode'] and identity['st_dev']==g['dev'] and identity['st_nlink']==12
 paths=[str(canonical)]+[e['path'] for e in g['replacement_paths']]
 for path in paths:
  p=Path(path);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==g['sha256'] and b['st_ino']==g['canonical_inode'] and b['st_dev']==g['dev'] and b['st_size']==g['bytes'] and stat.S_IMODE(b['st_mode'])==0o600;after_records.append({'path':path,'sha256':h,'fullstat_before_hash':a,'fullstat_after_hash':b})
 group_receipts.append({'status':'complete','sha256':g['sha256'],'mode':g['mode'],'bytes':g['bytes'],'canonical':str(canonical),'canonical_inode_preserved':g['canonical_inode'],'logical_paths':12,'duplicate_independent_inodes_consolidated':11,'post_all_paths_rehashed':True,'new_nlink':12})
assert len(after_records)==24
save(S/'POST-GUARD-EXPLANATION.json',{'status':'all protection checks passed; original failed guard preserved','first_exit':(R/'out/evidence/consolidate-runtime-dso-storage-01.exit').read_text().strip(),'reason':'New fullstat capture added st_blksize absent from prior assessment; direct dictionary equality therefore failed despite every original field being exactly equal. Corrected check compares ALL prior fields and records the extra field, without relaxing any identity/content condition.','protected_latest_V30_checks':protected_checks,'no_additional_storage_mutation_in_this_receipt_resume':True})
free_before=None
result={'status':'complete storage-only consolidation; no evidence content or logical path deleted','groups':group_receipts,'logical_old_paths_retained':24,'duplicate_independent_inodes_consolidated':22,'canonical_inodes_preserved':2,'before_current_SHA_and_fullstat':fresh_before,'after_current_SHA_and_fullstat':after_records,'prior_original_physical_metadata_preserved_in_fixed_receipt':fixed,'physical_inode_nlink_ctime_mtime_changes_are_intentional_and_reported':True,'original_logical_path_bytes_mode0600_preserved':True,'protected_latest_V30_four_paths_all_original_fullstat_fields_unchanged':True,'protected_current_SDK_fullstats_unchanged':True,'no_target_SDK_or_other_raw_file_linked_or_chmodded':True,'free_bytes_after':shutil.disk_usage('/workspace').free,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'new_build_test_native_measurement_executions':0,'post_guard_schema_error_record':'POST-GUARD-EXPLANATION.json'};save(S/'result.json',result);rel=BASE+'-result';dest=A/rel;dest.mkdir()
for name in ['result.json','journal.jsonl','PRESERVATION-FRESH.json','operations.jsonl','POST-GUARD-EXPLANATION.json']:shutil.copy2(S/name,dest/name)
for p in [Path('/tmp/consolidate_runtime_dso_storage_01.py'),Path(__file__),R/'out/evidence/consolidate-runtime-dso-storage-01.log',R/'out/evidence/consolidate-runtime-dso-storage-01.exit']:shutil.copy2(p,dest/p.name)
(dest/'README.md').write_text('# Exact old raw runtime snapshot storage consolidation\n\nAll24old logical paths retain complete bytes and mode0600; each was rehashed before and after. Two old raw canonicals retain their original inode;22duplicate physical inodes intentionally become links to their equal canonical. Original fullstat/inode/nlink/mtime/ctime remain fixed historical observations, not falsely described as unchanged physical identity. Each group held original inodes through complete verification, then removed only its own rollback aliases. Current SDK/target and four newestV30 paths were excluded and all originally captured protection fields remain exactly equal. First final-check failure was a fullstat schema key-count mismatch (new st_blksize field); corrected all-original-field comparison and preserved original failure are included. No raw content/path, source, executable, test or measurement result was deleted or rewritten. Storage equality makes no new timing/gate acceptance.\n')
final,fresh=publish(dest,rel,'Record verified raw DSO storage hardlinks and complete historical physical metadata');assert not git('status','--porcelain');save(S/'PUBLISHED.json',{'status':'complete','fixed_preconditions_commit':fixed,'preconditions_path':BASE,'fixed_result_commit':final,'result_path':rel,'logical_paths_preserved':24,'duplicate_physical_inodes_consolidated':22,'estimated_allocated_bytes_reclaimed':assessment['estimated_reclaimable_allocated_bytes'],'free_bytes':shutil.disk_usage('/workspace').free,'result_actual_fresh_HTTPS':fresh,'QUIET':True});Path('/tmp/full-push-runtime-dso-storage-consolidation.json').write_text(json.dumps({k:v for k,v in json.loads((S/'PUBLISHED.json').read_text()).items() if k!='result_actual_fresh_HTTPS'},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)

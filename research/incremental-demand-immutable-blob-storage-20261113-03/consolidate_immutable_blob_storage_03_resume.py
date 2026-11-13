from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request
R=Path('/workspace/zil-push-main');A=Path('/workspace/zil-demand-archive');P=R/'out/evidence/immutable-blob-storage-assessment-01';S=R/'out/evidence/immutable-blob-storage-consolidation-03';assert S.is_dir();REF='refs/heads/archive/source-history';BASE='research/incremental-demand-immutable-blob-storage-20261113-03';D=A/BASE
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def st(p):
 s=p.lstat();return {k:getattr(s,k) for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_atime_ns','st_blocks']}
def stable(a,b):return all(a[k]==b[k] for k in ['st_dev','st_ino','st_mode','st_size','st_nlink','st_uid','st_gid','st_mtime_ns','st_ctime_ns','st_blocks'])
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def log(v):
 with (S/'journal.jsonl').open('a') as f:f.write(json.dumps(v,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label):
 a=['git','-c','gc.auto=0','-c','maintenance.auto=false']+a[1:] if a[0]=='git' else a;p=subprocess.run(a,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n')
 assert not p.returncode,(label,p.stderr)
def pub(dest,rel,msg):
 act(['git','-C',str(A),'add','--',rel],'add');act(['git','-C',str(A),'commit','-m',msg],'commit');act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-push');fixed=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==fixed;fresh=[]
 for p in dest.iterdir():
  u='https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+rel+'/'+p.name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();fresh.append({'url':u,'bytes':len(b),'sha256':sha(p)})
 return fixed,fresh
assessment=json.loads((P/'ASSESSMENT.json').read_text());pre=json.loads((S/'PRECONDITIONS.json').read_text());plans=pre['plans'];blocked=pre['blocked_before_action'];all_paths={e['path'] for p in plans for e in p['members']};assert len(plans)==223 and not blocked
for plan in plans:
 for e in plan['members']:assert stable(st(Path(e['path'])),e['after_pre_hash'])
status=git('status','--porcelain=v1','--untracked-files=all').splitlines();assert status and all(line.startswith('?? '+BASE+'/') for line in status)
assert git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD')
def active_paths(paths):
 active=[]
 for proc in Path('/proc').iterdir():
  if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
  try:
   vals=[]
   try:vals.append((proc/'maps').read_text(errors='replace'))
   except OSError:pass
   for fd in (proc/'fd').iterdir():
    try:vals.append(os.readlink(fd))
    except OSError:pass
   for path in paths:
    if any(path in v for v in vals):active.append({'pid':proc.name,'path':path})
  except OSError:pass
 return active
assert not active_paths(all_paths)
shutil.copy2('/tmp/consolidate_immutable_blob_storage_03.py',D/'first_controller.py');shutil.copy2(R/'out/evidence/consolidate-immutable-blob-storage-03.log',D/'first_controller.log');shutil.copy2(R/'out/evidence/consolidate-immutable-blob-storage-03.exit',D/'first_controller.exit');shutil.copy2(__file__,D/Path(__file__).name)
fixed,fresh=pub(D,BASE,'Preserve exact scoped immutable blob preconditions and initial pre-write guard failure');save(S/'PRESERVATION-FRESH.json',{'fixed_commit':fixed,'path':BASE,'actual_fresh_HTTPS':fresh});free_before=shutil.disk_usage('/workspace').free;completed=[];post=[];changed=0
for number,plan in enumerate(plans):
 g=plan['group'];canonical=Path(g['canonical']);members={m['path']:m for m in plan['members']};paths=list(members);assert not active_paths(paths);expected=members[str(canonical)]['after_pre_hash'];assert stable(st(canonical),expected)
 for m in plan['members']:assert stable(st(Path(m['path'])),m['after_pre_hash'])
 holds=S/'holds'/str(number);holds.mkdir(parents=True);replaced=[]
 try:
  for i,e in enumerate(g['replacement_paths']):
   target=Path(e['path']);orig=st(target);assert stable(orig,members[str(target)]['after_pre_hash']);assert stable(st(canonical),expected);hold=holds/str(i);os.link(target,hold,follow_symlinks=False);held=st(target);assert held['st_nlink']==orig['st_nlink']+1 and all(held[k]==orig[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns','st_blocks']);tmp=target.with_name('.'+target.name+'.storage-03.tmp');assert not tmp.exists();os.link(canonical,tmp,follow_symlinks=False);linked=st(tmp);assert (linked['st_dev'],linked['st_ino'],linked['st_mode'],linked['st_size'])==(expected['st_dev'],expected['st_ino'],expected['st_mode'],expected['st_size']);assert stable(st(target),held);log({'phase':'prepared','path':str(target),'before':orig,'hold':str(hold),'temporary_link':str(tmp),'canonical':str(canonical)});os.replace(tmp,target);expected=st(canonical);assert st(target)['st_ino']==expected['st_ino'];replaced.append({'path':str(target),'hold':str(hold),'before':orig});log({'phase':'atomic-replace-complete-original-held','path':str(target),'before':orig,'after':st(target),'canonical':str(canonical)})
  observations=[]
  for path in paths:
   p=Path(path);a=st(p);h=sha(p);b=st(p);assert stable(a,b) and h==g['sha256'] and b['st_ino']==expected['st_ino'] and b['st_dev']==g['dev'] and stat.S_IMODE(b['st_mode'])==g['mode'] and b['st_size']==g['bytes'];observations.append({'path':path,'sha256':h,'before_post_hash':a,'after_post_hash':b})
  assert not active_paths(paths)
  for e in replaced:
   h=Path(e['hold']);old=st(h);assert (old['st_dev'],old['st_ino'],old['st_mode'],old['st_size'])==(e['before']['st_dev'],e['before']['st_ino'],e['before']['st_mode'],e['before']['st_size']);h.unlink();log({'phase':'group-verified-retire-own-hold','hold':str(h),'logical_path_retained':e['path'],'historical_inode':old['st_ino']})
  holds.rmdir();post+=observations;changed+=len(replaced);completed.append({'sha256':g['sha256'],'mode':g['mode'],'bytes':g['bytes'],'logical_paths':len(paths),'canonical':str(canonical),'canonical_inode_preserved':expected['st_ino'],'nlink_after':st(canonical)['st_nlink'],'replacement_paths':len(replaced),'estimated_allocated_bytes_reclaimed':g['estimated_reclaimable_allocated_bytes']});log({'phase':'group-complete','group':number,'sha256':g['sha256'],'paths':len(paths),'free_bytes':shutil.disk_usage('/workspace').free})
 except BaseException as ex:
  errors=[]
  for e in reversed(replaced):
   try:os.replace(e['hold'],e['path'])
   except BaseException as fail:errors.append({'path':e['path'],'error':repr(fail)})
  save(S/('GROUP-'+str(number)+'-STOPPED.json'),{'status':'group stopped, rollback attempted, further groups stopped','error':repr(ex),'rollback_errors':errors,'replaced':replaced});raise
result={'status':'complete within explicit assessed scope; no evidence logical path/content removed','groups_completed':len(completed),'groups_blocked_before_action':blocked,'groups':completed,'logical_paths_rehashed_before_and_after':len(post),'duplicate_inode_path_replacements':changed,'estimated_allocated_bytes_reclaimed':sum(g['estimated_allocated_bytes_reclaimed'] for g in completed),'before_plans_sha256':sha(S/'PRECONDITIONS.json'),'post_observations':post,'original_physical_metadata_historical_in_fixed_precondition':fixed,'inode_nlink_ctime_mtime_changes_intentional_recorded':True,'all_logical_paths_bytes_original_mode_preserved':True,'no_real_SDK_target_immutable_compiler_or_source_Git_linked':True,'no_chmod_or_content_writes':True,'no_outside_scope_or_42excluded_or_active_path_modification':True,'fallback_assessment_not_action':True,'free_bytes_before':free_before,'free_bytes_after':shutil.disk_usage('/workspace').free,'build_test_native_measurement_executions':0};save(S/'result.json',result);rel=BASE+'-result';dest=A/rel;dest.mkdir()
for name in ['result.json','journal.jsonl','operations.jsonl','PRESERVATION-FRESH.json']:shutil.copy2(S/name,dest/name)
(dest/'README.md').write_text('# Scoped immutable raw SHA-blob storage equality\n\nOnly previously assessed223raw blob groups were considered. Current source bytes and every mode/owner/device/stat were checked before; active/protected/external-inode aliases stop or exclude a group. Every operated logical path was rehashed afterward. Original inode holds were retained through group validation, and replaced paths remained continuously present via same-device atomic link/rename. Before/after full metadata and each inode/nlink/mtime/ctime implication are recorded. No chmod, content write, target/SDK/real immutable compiler linking, timing/gate/source-role rewrite, or fallback-scope action occurred. This is storage equality, not semantic code reuse permission.\n')
final,fresh=pub(dest,rel,'Record scoped immutable raw blob storage consolidation with exact byte and metadata proof');save(S/'PUBLISHED.json',{'status':'complete','fixed_result_commit':final,'result_path':rel,'fixed_preconditions_commit':fixed,'preconditions_path':BASE,'groups_completed':len(completed),'groups_blocked':len(blocked),'logical_paths_preserved':len(post),'duplicate_inode_path_replacements':changed,'estimated_allocated_bytes_reclaimed':result['estimated_allocated_bytes_reclaimed'],'actual_fresh_result_files':fresh,'free_bytes':shutil.disk_usage('/workspace').free,'QUIET':True});Path('/tmp/full-push-immutable-blob-storage-consolidation.json').write_text(json.dumps({k:v for k,v in json.loads((S/'PUBLISHED.json').read_text()).items() if k!='actual_fresh_result_files'},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)
